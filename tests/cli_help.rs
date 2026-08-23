use assert_cmd::Command;

#[test]
fn help_exits_zero() {
    let mut cmd = Command::cargo_bin("game-library").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("registry"))
        .stdout(predicates::str::contains("uninstall"))
        .stdout(predicates::str::contains("doctor"));
}
