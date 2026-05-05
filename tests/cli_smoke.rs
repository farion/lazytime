use std::process::Command;

#[test]
fn cli_help_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_lazytime"))
        .arg("--help")
        .output()
        .expect("failed to run lazytime --help");
    assert!(output.status.success());
}
