use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("refusing unapproved SSH alias: {0}")]
    UnapprovedAlias(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub const DEFAULT_SSH: &str = "ssh";

pub fn ssh_helper() -> PathBuf {
    crate::config::resolved().ssh_helper
}

pub fn run(
    alias: &str,
    argv: &[&str],
    allowed_aliases: &[&str],
    helper: Option<&Path>,
) -> Result<Output, Error> {
    if !allowed_aliases.contains(&alias) {
        return Err(Error::UnapprovedAlias(alias.to_string()));
    }
    Ok(Command::new(helper.unwrap_or(&ssh_helper()))
        .args(["-o", "BatchMode=yes", alias, "--", &quote_command(argv)])
        .output()?)
}

pub fn push(
    alias: &str,
    dest: &str,
    contents: &[u8],
    allowed_aliases: &[&str],
    helper: Option<&Path>,
) -> Result<(), Error> {
    if !allowed_aliases.contains(&alias) {
        return Err(Error::UnapprovedAlias(alias.to_string()));
    }
    let remote = format!("tee {}", quote_command(&[dest]));
    let mut child = Command::new(helper.unwrap_or(&ssh_helper()))
        .args(["-o", "BatchMode=yes", alias, "--", &remote])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(contents)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(Error::Io(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )));
    }
    Ok(())
}

fn quote_command(argv: &[&str]) -> String {
    argv.iter()
        .map(|part| format!("'{}'", part.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn session_prefix(uid: u32, display: &str, wayland: &str) -> Vec<String> {
    vec![
        format!("XDG_RUNTIME_DIR=/run/user/{uid}"),
        format!("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/{uid}/bus"),
        format!("DISPLAY={display}"),
        format!("WAYLAND_DISPLAY={wayland}"),
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/bin:/bin".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::quote_command;

    #[test]
    fn quotes_spaces_and_colons() {
        assert_eq!(
            quote_command(&[
                "/home/alice/.local/bin/ludusavi-lutris-wrap",
                "status",
                "--name",
                "Divinity: Original Sin II - Definitive Edition",
            ]),
            "'/home/alice/.local/bin/ludusavi-lutris-wrap' 'status' '--name' 'Divinity: Original Sin II - Definitive Edition'"
        );
    }
}
