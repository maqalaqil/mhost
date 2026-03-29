use std::process::Command;

fn mhost_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mhost"))
}

#[test]
fn test_version() {
    let output = mhost_bin().arg("--version").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mhost"));
}

#[test]
fn test_help() {
    let output = mhost_bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("start"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("logs"));
}

#[test]
fn test_completion_bash() {
    let output = mhost_bin()
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mhost"));
}

#[test]
fn test_completion_zsh() {
    let output = mhost_bin()
        .args(["completion", "zsh"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn test_completion_fish() {
    let output = mhost_bin()
        .args(["completion", "fish"])
        .output()
        .unwrap();
    assert!(output.status.success());
}
