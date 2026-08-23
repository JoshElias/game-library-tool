use serde::{Deserialize, Serialize};

use crate::inventory::GamingHost;
use crate::status::{has_graphical_session, Remote};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DoctorReport {
    pub host: String,
    pub alias: String,
    pub desktop_user: String,
    pub reachable: bool,
    pub identity_ok: bool,
    pub graphical_session: bool,
    pub lutris_version: String,
    pub ludusavi_version: String,
    pub wrap_present: bool,
    pub xdg_ok: bool,
    pub notes: Vec<String>,
}

pub fn inspect(host: &GamingHost, remote: &dyn Remote) -> DoctorReport {
    let (id_code, id_out, _) = remote.run(&["id", "-un"]);
    let live_user = id_out.lines().next().unwrap_or("").trim().to_string();
    let identity_ok = id_code == 0 && live_user == host.desktop_user;
    let lutris_version = first_line(remote.run(&["lutris", "--version"]));
    let ludusavi_version = first_line(remote.run(&["ludusavi", "--version"]));
    let wrap = format!(
        "/home/{}/.local/bin/ludusavi-lutris-wrap",
        host.desktop_user
    );
    let wrap_present = remote.run(&["test", "-x", &wrap]).0 == 0;
    let data_ok = remote.run(&["test", "-d", &host.xdg_data_home]).0 == 0;
    let gog_ok = remote.run(&["test", "-d", &host.gog_library_root]).0 == 0;
    let xdg_ok = data_ok && gog_ok;
    let graphical_session = has_graphical_session(remote);
    let reachable = id_code == 0 && !lutris_version.is_empty();
    let mut notes = Vec::new();
    if !reachable {
        notes.push("unreachable or no session; do not treat games as missing".into());
    }
    if id_code == 0 && !identity_ok {
        notes.push(format!(
            "desktop user mismatch: live={live_user} expected={}",
            host.desktop_user
        ));
    }
    if reachable && !graphical_session {
        notes.push("no graphical session".into());
    }
    if reachable && !wrap_present {
        notes.push("ludusavi-lutris-wrap missing".into());
    }
    if reachable && !xdg_ok {
        notes.push("configured XDG/GOG path is missing on the host".into());
    }
    if lutris_version.is_empty() && !wrap_present {
        notes.push("empty Lutris/wrap probe is usually SSH or session failure".into());
    }
    DoctorReport {
        host: host.name.clone(),
        alias: host.alias.clone(),
        desktop_user: host.desktop_user.clone(),
        reachable,
        identity_ok,
        graphical_session,
        lutris_version,
        ludusavi_version,
        wrap_present,
        xdg_ok,
        notes,
    }
}

fn first_line(result: (i32, String, String)) -> String {
    if result.0 != 0 {
        return String::new();
    }
    result.1.lines().next().unwrap_or("").trim().to_string()
}

pub fn format_report(report: &DoctorReport) -> String {
    let mut lines = vec![
        format!("host: {}", report.host),
        format!("alias: {}", report.alias),
        format!("desktop_user: {}", report.desktop_user),
        format!("reachable: {}", report.reachable),
        format!("identity_ok: {}", report.identity_ok),
        format!("graphical_session: {}", report.graphical_session),
        format!("lutris: {}", empty_as_dash(&report.lutris_version)),
        format!("ludusavi: {}", empty_as_dash(&report.ludusavi_version)),
        format!(
            "wrap: {}",
            if report.wrap_present {
                "present"
            } else {
                "missing"
            }
        ),
        format!("xdg_ok: {}", report.xdg_ok),
    ];
    if report.notes.is_empty() {
        lines.push("notes: none".into());
    } else {
        lines.push("notes:".into());
        for note in &report.notes {
            lines.push(format!("  - {note}"));
        }
    }
    lines.join("\n")
}

fn empty_as_dash(value: &str) -> &str {
    if value.is_empty() {
        "-"
    } else {
        value
    }
}
