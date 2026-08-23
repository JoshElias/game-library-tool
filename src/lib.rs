use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::{ArgGroup, Parser, Subcommand};

pub mod art;
pub mod config;
pub mod confirm;
pub mod doctor;
pub mod download;
pub mod hosts_file;
pub mod install;
pub mod inventory;
pub mod legacy;
pub mod paths;
pub mod registry;
pub mod ssh;
pub mod status;
pub mod store;
pub mod uninstall;
pub mod wrap;

use inventory::{load_host, load_hosts};
use status::{collect_status, Remote};
use store::RegistryStore;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("cancelled")]
    ClapHelp,
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::ClapHelp => 0,
            Error::Message(_) => 2,
        }
    }
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "game-library",
    about = "Lutris and Ludusavi game-library operator"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List configured hosts
    Hosts,
    /// Maintain pinned game recipes
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
    /// Read-only host and recipe status
    Status {
        slug: String,
        #[arg(long)]
        host: String,
        #[arg(long)]
        json: bool,
    },
    /// Install a pinned recipe on a host
    #[command(group(ArgGroup::new("target").required(true).args(["host", "all"])))]
    Install {
        slug: String,
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        all: bool,
        /// Unique committed lineage root when this host is not the first enroll
        #[arg(long)]
        import_root: Option<String>,
    },
    /// Check post-launch save lineage
    Verify {
        slug: String,
        #[arg(long)]
        host: String,
    },
    /// Trash one installed GOG/Lutris game; keep library + lineage
    Uninstall {
        slug: String,
        #[arg(long)]
        host: String,
    },
    /// Read-only host self-check
    Doctor {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    /// List registered games
    List,
    /// Show one recipe
    Show { slug: String },
    /// Add a recipe, prompting one field at a time
    Add {
        #[arg(long)]
        force: bool,
    },
    /// Remove a recipe (does not uninstall)
    Remove { slug: String },
}

struct SshRemote {
    alias: String,
}

impl Remote for SshRemote {
    fn run(&self, argv: &[&str]) -> (i32, String, String) {
        let lutris = argv.first().is_some_and(|cmd| *cmd == "lutris");
        let python = argv.first().is_some_and(|cmd| *cmd == "python3");
        let gdbus = argv.first().is_some_and(|cmd| *cmd == "gdbus");
        let script;
        let effective: Vec<&str> = if lutris || python || gdbus {
            script = format!(
                "uid=$(id -u); export XDG_RUNTIME_DIR=/run/user/$uid; export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus; export DISPLAY=\"${{DISPLAY:-:0}}\"; export WAYLAND_DISPLAY=\"${{WAYLAND_DISPLAY:-wayland-1}}\"; eval \"$(systemctl --user show-environment 2>/dev/null | grep -E '^(DISPLAY|WAYLAND_DISPLAY)=' || true)\"; PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:/bin {}",
                argv.iter()
                    .map(|part| shlex_quote(part))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            vec!["bash", "-c", &script]
        } else {
            argv.to_vec()
        };
        match ssh::run(&self.alias, &effective, &[&self.alias], None) {
            Ok(output) => (
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ),
            Err(error) => (1, String::new(), error.to_string()),
        }
    }

    fn write_file(&self, dest: &str, contents: &[u8]) -> Result<(), String> {
        ssh::push(&self.alias, dest, contents, &[&self.alias], None)
            .map_err(|error| error.to_string())
    }
}

struct LocalRemote;

impl Remote for LocalRemote {
    fn run(&self, argv: &[&str]) -> (i32, String, String) {
        let lutris = argv.first().is_some_and(|cmd| *cmd == "lutris");
        let python = argv.first().is_some_and(|cmd| *cmd == "python3");
        let gdbus = argv.first().is_some_and(|cmd| *cmd == "gdbus");
        let script;
        let effective: Vec<&str> = if lutris || python || gdbus {
            script = format!(
                "uid=$(id -u); export XDG_RUNTIME_DIR=/run/user/$uid; export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus; export DISPLAY=\"${{DISPLAY:-:0}}\"; export WAYLAND_DISPLAY=\"${{WAYLAND_DISPLAY:-wayland-1}}\"; eval \"$(systemctl --user show-environment 2>/dev/null | grep -E '^(DISPLAY|WAYLAND_DISPLAY)=' || true)\"; PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:/bin {}",
                argv.iter()
                    .map(|part| shlex_quote(part))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            vec!["bash", "-c", &script]
        } else {
            argv.to_vec()
        };
        match std::process::Command::new(effective[0])
            .args(&effective[1..])
            .output()
        {
            Ok(output) => (
                output.status.code().unwrap_or(1),
                String::from_utf8_lossy(&output.stdout).into_owned(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ),
            Err(error) => (1, String::new(), error.to_string()),
        }
    }

    fn write_file(&self, dest: &str, contents: &[u8]) -> Result<(), String> {
        if let Some(parent) = Path::new(dest).parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(dest, contents).map_err(|error| error.to_string())
    }
}

fn connect(host: &inventory::GamingHost) -> Box<dyn Remote> {
    let user = crate::paths::current_username().unwrap_or_default();
    let hostname = crate::paths::current_hostname().unwrap_or_default();
    if crate::paths::is_local_host(&host.name, &host.desktop_user, &user, &hostname) {
        Box::new(LocalRemote)
    } else {
        Box::new(SshRemote {
            alias: host.alias.clone(),
        })
    }
}

fn shlex_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn run<I, T>(args: I) -> Result<(), Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                Err(Error::Message(String::new()))
            } else {
                Err(Error::ClapHelp)
            };
        }
    };
    match cli.command {
        Command::Hosts => hosts_command(),
        Command::Registry { command } => registry_command(command),
        Command::Status { slug, host, json } => status_command(&slug, &host, json),
        Command::Install {
            slug,
            host,
            all,
            import_root,
        } => install_command(&slug, host.as_deref(), all, import_root.as_deref()),
        Command::Verify { slug, host } => verify_command(&slug, &host),
        Command::Uninstall { slug, host } => uninstall_command(&slug, &host),
        Command::Doctor { host, json } => doctor_command(host.as_deref(), json),
    }
}

fn hosts_command() -> Result<(), Error> {
    let hosts = load_hosts().map_err(|error| error.to_string())?;
    for host in hosts {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            host.name, host.alias, host.desktop_user, host.gog_library_root, host.game_library_root
        );
    }
    Ok(())
}

fn registry_command(command: RegistryCommand) -> Result<(), Error> {
    let store = RegistryStore::new(RegistryStore::default_root());
    match command {
        RegistryCommand::List => {
            let recipes = store.list().map_err(store_err)?;
            if recipes.is_empty() {
                println!("no registered games");
                return Ok(());
            }
            for recipe in recipes {
                println!(
                    "{}\t{:?}\t{}\tinstallable={}",
                    recipe.slug,
                    recipe.source,
                    recipe.title,
                    if recipe.installable { "yes" } else { "no" }
                );
            }
            Ok(())
        }
        RegistryCommand::Show { slug } => {
            let recipe = store.show(&slug).map_err(store_err)?;
            print!(
                "{}",
                serde_yaml::to_string(&recipe).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        RegistryCommand::Add { force } => {
            let recipe = store::prompt_new_recipe(
                |prompt| {
                    print!("{prompt}");
                    let _ = io::stdout().flush();
                    let mut line = String::new();
                    io::stdin().read_line(&mut line)?;
                    Ok(line)
                },
                io::stdout(),
            )
            .map_err(store_err)?;
            store.add(&recipe, force).map_err(store_err)?;
            println!("added {} installable={}", recipe.slug, recipe.installable);
            Ok(())
        }
        RegistryCommand::Remove { slug } => {
            print!("type the slug again to remove: ");
            let _ = io::stdout().flush();
            let mut confirm = String::new();
            io::stdin()
                .read_line(&mut confirm)
                .map_err(|error| error.to_string())?;
            store.remove(&slug, confirm.trim()).map_err(store_err)?;
            println!("removed {slug}");
            Ok(())
        }
    }
}

fn status_command(slug: &str, host_name: &str, json: bool) -> Result<(), Error> {
    let host = load_host(host_name).map_err(|error| error.to_string())?;
    let recipe = RegistryStore::new(RegistryStore::default_root())
        .show(slug)
        .map_err(store_err)?;
    let remote = connect(&host);
    let status = collect_status(&host, &recipe, remote.as_ref());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&status).map_err(|error| error.to_string())?
        );
    } else {
        println!(
            "{}",
            serde_yaml::to_string(&status).map_err(|error| error.to_string())?
        );
    }
    if status.legacy {
        Err(Error::Message(String::new()))
    } else {
        Ok(())
    }
}

fn install_command(
    slug: &str,
    host_name: Option<&str>,
    all: bool,
    import_root: Option<&str>,
) -> Result<(), Error> {
    let recipe = RegistryStore::new(RegistryStore::default_root())
        .show(slug)
        .map_err(store_err)?;
    let hosts = if all {
        load_hosts().map_err(|error| error.to_string())?
    } else {
        vec![load_host(host_name.unwrap()).map_err(|error| error.to_string())?]
    };
    if all {
        let mut plans = Vec::new();
        for host in &hosts {
            let remote = connect(host);
            let status = collect_status(host, &recipe, remote.as_ref());
            plans.push(install::describe_plan(host, &recipe, &status));
        }
        confirm::require_go(
            |prompt| {
                print!("{prompt}");
                let _ = io::stdout().flush();
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                Ok(line)
            },
            io::stdout(),
            &plans.join("\n\n"),
        )
        .map_err(|error| error.to_string())?;
        for host in hosts {
            match install::install_game(
                &host,
                &recipe,
                connect(&host).as_ref(),
                |_| Ok(String::new()),
                io::stdout(),
                false,
                import_root,
            ) {
                Ok(message) => println!("{message}"),
                Err(install::Error::Legacy(error)) => println!("{error}"),
                Err(error) => return Err(Error::Message(error.to_string())),
            }
        }
        return Ok(());
    }
    let host = &hosts[0];
    let remote = connect(host);
    let message = install::install_game(
        host,
        &recipe,
        remote.as_ref(),
        |prompt| {
            print!("{prompt}");
            let _ = io::stdout().flush();
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            Ok(line)
        },
        io::stdout(),
        true,
        import_root,
    )
    .map_err(|error| error.to_string())?;
    println!("{message}");
    Ok(())
}

fn uninstall_command(slug: &str, host_name: &str) -> Result<(), Error> {
    let host = load_host(host_name).map_err(|error| error.to_string())?;
    let recipe = RegistryStore::new(RegistryStore::default_root())
        .show(slug)
        .map_err(store_err)?;
    let remote = connect(&host);
    let message = uninstall::uninstall_game(
        &host,
        &recipe,
        remote.as_ref(),
        |prompt| {
            print!("{prompt}");
            let _ = io::stdout().flush();
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            Ok(line)
        },
        io::stdout(),
        true,
    )
    .map_err(|error| error.to_string())?;
    println!("{message}");
    Ok(())
}

fn verify_command(slug: &str, host_name: &str) -> Result<(), Error> {
    let host = load_host(host_name).map_err(|error| error.to_string())?;
    let recipe = RegistryStore::new(RegistryStore::default_root())
        .show(slug)
        .map_err(store_err)?;
    let remote = connect(&host);
    let status = collect_status(&host, &recipe, remote.as_ref());
    if status.legacy {
        println!("legacy; skipped: {slug} on {}", host.name);
        return Err(Error::Message(String::new()));
    }
    if status.complete {
        println!(
            "pass: {slug} on {} generation={}",
            host.name,
            status.generation.unwrap_or_default()
        );
        Ok(())
    } else {
        println!(
            "fail: {slug} on {} complete={} enrolled={}",
            host.name, status.complete, status.enrolled
        );
        Err(Error::Message(String::new()))
    }
}

fn store_err(error: store::Error) -> Error {
    Error::Message(error.to_string())
}

fn doctor_command(host_name: Option<&str>, json: bool) -> Result<(), Error> {
    let hosts = if let Some(name) = host_name {
        vec![load_host(name).map_err(|error| error.to_string())?]
    } else {
        load_hosts().map_err(|error| error.to_string())?
    };
    let mut reports = Vec::new();
    let mut failed = false;
    for host in hosts {
        let remote = connect(&host);
        let report = doctor::inspect(&host, remote.as_ref());
        if !report.reachable || !report.identity_ok {
            failed = true;
        }
        reports.push(report);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&reports).map_err(|error| error.to_string())?
        );
    } else {
        for (index, report) in reports.iter().enumerate() {
            if index > 0 {
                println!();
            }
            println!("{}", doctor::format_report(report));
        }
    }
    if failed {
        Err(Error::Message(String::new()))
    } else {
        Ok(())
    }
}

pub fn main_from<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::ClapHelp) => ExitCode::SUCCESS,
        Err(error) => {
            if !matches!(error, Error::Message(ref message) if message.is_empty()) {
                eprintln!("{error}");
            }
            ExitCode::from(error.exit_code() as u8)
        }
    }
}
