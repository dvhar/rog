use std::io::Write;
use std::process::{Command, Stdio};

const TEST_INPUT: &str = r#"2023-11-12 20:48:30.241 INFO [src/main.rs:13] Starting up
2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff
2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
2023-11-12 20:48:30.245 DEBUG [src/lib.rs:10] Debug info here
2023-11-12 20:48:30.246 TRACE [src/lib.rs:11] Trace message
2023-11-12 20:48:30.247 INFO [src/main.rs:18] More work
2023-11-12 20:48:30.248 WARN [src/main.rs:19] Another warning
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.250 INFO [src/main.rs:21] Wrapping up
2023-11-12 20:48:30.251 DEBUG [src/lib.rs:12] Additional debug
2023-11-12 20:48:30.252 INFO [src/main.rs:22] All done
2023-11-12 20:48:30.253 TRACE [src/lib.rs:13] Something to trace
2023-11-12 20:48:30.254 INFO [src/main.rs:23] Initialization complete
2023-11-12 20:48:30.255 WARN [src/main.rs:24] Minor issue detected
2023-11-12 20:48:30.261 INFO [src/main.rs:28] Shutting down
2023-11-12 20:48:30.262 WARN [src/main.rs:29] Shutdown warning
2023-11-12 20:48:30.263 ERROR [src/main.rs:30] Shutdown error
2023-11-12 20:48:30.264 INFO [src/main.rs:31] Shutdown complete
2023-11-12 20:48:30.265 DEBUG [src/lib.rs:16] Final debug
2023-11-12 20:48:30.266 TRACE [src/lib.rs:17] Final trace
2023-11-12 20:48:30.267 INFO [src/main.rs:32] System halted
2023-11-12 20:48:30.268 WARN [src/main.rs:33] Post-shutdown warning
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error
"#;

fn run_test_stdin(args: &[&str], input: &str, expected_output: &str) {
    let exp = expected_output.trim();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rog"));
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let trimmed_output = stdout.trim();
    if exp != trimmed_output {
        eprintln!("Expected:\n{}\n\nGot:\n{}", exp, trimmed_output);
        panic!("test failed");
    }
}

#[test]
fn test_no_args() {
    run_test_stdin(&["-c"], TEST_INPUT, TEST_INPUT);
}

#[test]
fn test_simple_grep() {
    let expected = r#"2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff"#;
    run_test_stdin(&["-c", "-g", "stuff"], TEST_INPUT, expected);
}

#[test]
fn test_grep_bctx() {
    let expected = r#"2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff
2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
"#;
    run_test_stdin(&["-c", "-g", "Oups", "-B2"], TEST_INPUT, expected);
}

#[test]
fn test_grep_bctx_vgrep() {
    let expected = r#"2023-11-12 20:48:30.241 INFO [src/main.rs:13] Starting up
2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.247 INFO [src/main.rs:18] More work
2023-11-12 20:48:30.248 WARN [src/main.rs:19] Another warning
2023-11-12 20:48:30.253 TRACE [src/lib.rs:13] Something to trace
2023-11-12 20:48:30.254 INFO [src/main.rs:23] Initialization complete
2023-11-12 20:48:30.255 WARN [src/main.rs:24] Minor issue detected
2023-11-12 20:48:30.261 INFO [src/main.rs:28] Shutting down
2023-11-12 20:48:30.262 WARN [src/main.rs:29] Shutdown warning
2023-11-12 20:48:30.266 TRACE [src/lib.rs:17] Final trace
2023-11-12 20:48:30.267 INFO [src/main.rs:32] System halted
2023-11-12 20:48:30.268 WARN [src/main.rs:33] Post-shutdown warning
"#;
    run_test_stdin(&["-c", "-g", "WARN", "-B2", "-v", "Trace message"], TEST_INPUT, expected);
}

/// Vgrep-only: filter out lines matching a pattern (no grep, no bctx).
/// Exercises the vm_tail block at line 368:
///   if opts.bctx == 0 && !matches!(maybe_vre, None)
/// Tests Match + MatchJmp without Invert — the path I fixed for
/// ordering (Match before MatchJmp) and set_place targeting.
#[test]
fn test_vgrep_only() {
    // Filter out all TRACE lines
    let expected = r#"2023-11-12 20:48:30.241 INFO [src/main.rs:13] Starting up
2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff
2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
2023-11-12 20:48:30.245 DEBUG [src/lib.rs:10] Debug info here
2023-11-12 20:48:30.247 INFO [src/main.rs:18] More work
2023-11-12 20:48:30.248 WARN [src/main.rs:19] Another warning
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.250 INFO [src/main.rs:21] Wrapping up
2023-11-12 20:48:30.251 DEBUG [src/lib.rs:12] Additional debug
2023-11-12 20:48:30.252 INFO [src/main.rs:22] All done
2023-11-12 20:48:30.254 INFO [src/main.rs:23] Initialization complete
2023-11-12 20:48:30.255 WARN [src/main.rs:24] Minor issue detected
2023-11-12 20:48:30.261 INFO [src/main.rs:28] Shutting down
2023-11-12 20:48:30.262 WARN [src/main.rs:29] Shutdown warning
2023-11-12 20:48:30.263 ERROR [src/main.rs:30] Shutdown error
2023-11-12 20:48:30.264 INFO [src/main.rs:31] Shutdown complete
2023-11-12 20:48:30.265 DEBUG [src/lib.rs:16] Final debug
2023-11-12 20:48:30.267 INFO [src/main.rs:32] System halted
2023-11-12 20:48:30.268 WARN [src/main.rs:33] Post-shutdown warning
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error"#;
    run_test_stdin(&["-c", "-v", "TRACE"], TEST_INPUT, expected);
}

/// Grep with a pattern that matches nothing — exercises the skip_print
/// jump when nothing matches. The Invert+MatchJmp(skip_print) path
/// should cause every line to jump to the loop-back without printing.
#[test]
fn test_grep_no_match() {
    run_test_stdin(&["-c", "-g", "NONEXISTENT"], TEST_INPUT, "");
}

/// Vgrep that filters nothing (no lines match the vgrep pattern).
/// MatchJmp should never fire — every line prints.
#[test]
fn test_vgrep_no_filter() {
    run_test_stdin(&["-c", "-v", "NONEXISTENT"], TEST_INPUT, TEST_INPUT);
}

/// Grep + vgrep combined: grep selects lines, vgrep further excludes.
/// This exercises both the Invert+MatchJmp(skip_print) path for grep
/// AND the Match+MatchJmp(vjump) path for vgrep inside the bctx loop.
#[test]
fn test_grep_vgrep_no_bctx() {
    // grep ERROR, vgrep out "Shutdown error" → only first two ERROR lines
    let expected = r#"2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error"#;
    run_test_stdin(&["-c", "-g", "ERROR", "-v", "Shutdown"], TEST_INPUT, expected);
}
