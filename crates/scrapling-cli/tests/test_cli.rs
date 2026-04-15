use std::process::Command;

fn scrapling_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_scrapling"))
}

#[test]
fn cli_help() {
    let output = scrapling_bin().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("extract"));
    assert!(stdout.contains("info"));
}

#[test]
fn cli_info() {
    let output = scrapling_bin().arg("info").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("scrapling-rs"));
    assert!(stdout.contains("core"));
    assert!(stdout.contains("fetch"));
}

#[test]
fn cli_extract_help() {
    let output = scrapling_bin()
        .args(["extract", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("get"));
    assert!(stdout.contains("post"));
    assert!(stdout.contains("put"));
    assert!(stdout.contains("delete"));
}

#[test]
fn cli_extract_get_help() {
    let output = scrapling_bin()
        .args(["extract", "get", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--css-selector"));
    assert!(stdout.contains("--proxy"));
    assert!(stdout.contains("--timeout"));
    assert!(stdout.contains("--header"));
}
