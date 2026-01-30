use std::io::Write;
use std::process::{Command, Stdio};

const TEST_INPUT: &str = r#"2023-11-12 20:48:30.241 INFO [src/main.rs:13] Starting up
2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff
2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
"#;

#[test]
fn test_no_args() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rog")).arg("-c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = cmd.stdin.as_mut().unwrap();
    stdin.write_all(TEST_INPUT.as_bytes()).unwrap();

    let output = cmd.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.trim(), TEST_INPUT.trim());
}

#[test]
fn test_simple_grep() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rog")).arg("-c")
        .arg("-g")
        .arg("stuff")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdin = cmd.stdin.as_mut().unwrap();
    stdin.write_all(TEST_INPUT.as_bytes()).unwrap();

    let output = cmd.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = r#"2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff"#;
    assert_eq!(stdout.trim(), expected.trim());
}
