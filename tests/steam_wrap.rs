use std::path::PathBuf;
use std::process::Command;

#[test]
fn python_steam_wrap_helper_tests_pass() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/test_set_steam_wrap.py");
    let status = Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(&script)
        .status()
        .expect("python3");
    assert!(status.success(), "python steam-wrap helper tests failed");
}
