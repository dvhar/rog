use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

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

/// -w (word-boundary grep): substring "warn" should match "warning" with plain -g
#[test]
fn test_wgrep_substring_match() {
    let expected = r#"2023-11-12 20:48:30.241 WARN [src/main.rs:14] Just a warning
2023-11-12 20:48:30.248 WARN [src/main.rs:19] Another warning
2023-11-12 20:48:30.262 WARN [src/main.rs:29] Shutdown warning
2023-11-12 20:48:30.268 WARN [src/main.rs:33] Post-shutdown warning"#;
    run_test_stdin(&["-c", "-g", "warn"], TEST_INPUT, expected);
}

/// -w (word-boundary grep): "warn" as a whole word does not appear in input,
/// so -w "warn" should match nothing (word boundaries prevent "warning" from matching)
#[test]
fn test_wgrep_no_partial_match() {
    run_test_stdin(&["-c", "-w", "warn"], TEST_INPUT, "");
}

/// -A (after context): prints N lines after each grep match
#[test]
fn test_grep_actx() {
    let expected = r#"2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.250 INFO [src/main.rs:21] Wrapping up
2023-11-12 20:48:30.263 ERROR [src/main.rs:30] Shutdown error
2023-11-12 20:48:30.264 INFO [src/main.rs:31] Shutdown complete
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error"#;
    run_test_stdin(&["-c", "-g", "ERROR", "-A1"], TEST_INPUT, expected);
}

/// -A with overlapping context: when a match appears within the context window
/// of a previous match, the counter resets and we get N new lines after it
#[test]
fn test_grep_actx_overlap() {
    // 4 ERRORs total. The 4th (Post-shutdown error) is outside any context window.
    let expected = r#"2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
2023-11-12 20:48:30.245 DEBUG [src/lib.rs:10] Debug info here
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.250 INFO [src/main.rs:21] Wrapping up
2023-11-12 20:48:30.251 DEBUG [src/lib.rs:12] Additional debug
2023-11-12 20:48:30.263 ERROR [src/main.rs:30] Shutdown error
2023-11-12 20:48:30.264 INFO [src/main.rs:31] Shutdown complete
2023-11-12 20:48:30.265 DEBUG [src/lib.rs:16] Final debug
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error"#;
    run_test_stdin(&["-c", "-g", "ERROR", "-A2"], TEST_INPUT, expected);
}

/// -C (context): activates both before-context (-B) and after-context (-A)
/// with the same line count, showing surrounding lines for each match
#[test]
fn test_grep_context() {
    let expected = r#"2023-11-12 20:48:30.242 INFO [src/main.rs:15] Doing stuff
2023-11-12 20:48:30.243 ERROR [src/main.rs:16] Oups, an error
2023-11-12 20:48:30.244 INFO [src/main.rs:17] Done
2023-11-12 20:48:30.248 WARN [src/main.rs:19] Another warning
2023-11-12 20:48:30.249 ERROR [src/main.rs:20] Second error
2023-11-12 20:48:30.250 INFO [src/main.rs:21] Wrapping up
2023-11-12 20:48:30.262 WARN [src/main.rs:29] Shutdown warning
2023-11-12 20:48:30.263 ERROR [src/main.rs:30] Shutdown error
2023-11-12 20:48:30.264 INFO [src/main.rs:31] Shutdown complete
2023-11-12 20:48:30.268 WARN [src/main.rs:33] Post-shutdown warning
2023-11-12 20:48:30.269 ERROR [src/main.rs:34] Post-shutdown error"#;
    run_test_stdin(&["-c", "-g", "ERROR", "-C1"], TEST_INPUT, expected);
}

/// -o (width): truncates lines to specified byte width, not cutting mid-unicode
#[test]
fn test_width_truncate() {
    let input = "This is a very long line that exceeds the limit\nShort line\n";
    // "This is a very lon" is exactly 18 chars, truncated from longer line
    let expected = "This is a very lon\nShort line";
    run_test_stdin(&["-c", "-o18"], input, expected);
}

/// FIFO: create a named pipe, write data to it, verify output, and confirm cleanup.
/// This tests vm_fifo end-to-end: fifo creation, reading, and ctrlc-triggered deletion.
#[test]
fn test_fifo_create_read_cleanup() {
    let fifo_path = "/tmp/rog_test_fifo_".to_string() + &std::process::id().to_string();

    // Ensure any stale file is gone
    let _ = std::fs::remove_file(&fifo_path);
    assert!(!std::path::Path::new(&fifo_path).exists());

    let child = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-f", &fifo_path, "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Give rog time to create the fifo
    thread::sleep(Duration::from_millis(500));

    // Verify the fifo was created
    assert!(
        std::path::Path::new(&fifo_path).exists(),
        "fifo should exist after rog starts"
    );
    let metadata = std::fs::metadata(&fifo_path).unwrap();
    assert!(
        metadata.file_type().is_fifo(),
        "path should be a named pipe, not a regular file"
    );

    // Write data to the fifo and close the writer (triggers EOF)
    let test_data = "hello from fifo\nsecond line\n";
    let mut fifo_writer = std::fs::File::create(&fifo_path).unwrap();
    fifo_writer.write_all(test_data.as_bytes()).unwrap();
    drop(fifo_writer); // closing writer causes EOF on the reader side

    // Give rog time to read the data
    thread::sleep(Duration::from_millis(500));

    // Send SIGINT to trigger the ctrlc handler, which removes the fifo
    let kill_result = Command::new("kill")
        .args(&["-INT", &child.id().to_string()])
        .output()
        .unwrap();
    assert!(kill_result.status.success(), "kill -INT failed");

    // Wait for rog to exit
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "rog exited with error: {:?}", String::from_utf8_lossy(&output.stderr));

    // Verify output contains the data we wrote
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("hello from fifo"), "output missing 'hello from fifo': {}", stdout);
    assert!(stdout.contains("second line"), "output missing 'second line': {}", stdout);

    // Verify the fifo was cleaned up
    assert!(
        !std::path::Path::new(&fifo_path).exists(),
        "fifo should be removed after rog exits"
    );
}
