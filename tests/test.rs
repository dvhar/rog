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

/// Client/Server: start a rog server with fifo + -s, connect a client with -k,
/// write data to the fifo, and verify the client receives the output.
/// This tests the full pipeline: accept_clients, ServerWrite op, ReadSocket op.
#[test]
fn test_server_fifo_client() {
    let fifo_path = format!("/tmp/rog_test_server_fifo_{}", std::process::id());
    let port = 19901;

    // Ensure stale fifo is gone
    let _ = std::fs::remove_file(&fifo_path);

    // Start server in fifo mode with -s (server mode)
    let server = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-s", "-f", &fifo_path, "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Give server time to create fifo and bind to port
    thread::sleep(Duration::from_millis(500));

    // Verify the fifo was created
    assert!(
        std::path::Path::new(&fifo_path).exists(),
        "fifo should exist after server starts"
    );

    // Start client connected to localhost server
    let client = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Give client time to connect
    thread::sleep(Duration::from_millis(500));

    // Write data to the fifo
    let test_data = "server_mode_test_line\nsecond_server_line\n";
    let mut fifo_writer = std::fs::File::create(&fifo_path).unwrap();
    fifo_writer.write_all(test_data.as_bytes()).unwrap();
    drop(fifo_writer); // closing writer causes EOF, rog will re-open fifo

    // Give time for data to flow through server to client
    thread::sleep(Duration::from_millis(500));

    // Kill the client first to flush its output
    let kill_result = Command::new("kill")
        .args(&["-INT", &client.id().to_string()])
        .output()
        .unwrap();
    assert!(kill_result.status.success(), "kill client failed");
    let client_output = client.wait_with_output().unwrap();

    // Verify client received the data
    let client_stdout = String::from_utf8(client_output.stdout).unwrap();
    assert!(
        client_stdout.contains("server_mode_test_line"),
        "client output missing 'server_mode_test_line': {}",
        client_stdout
    );
    assert!(
        client_stdout.contains("second_server_line"),
        "client output missing 'second_server_line': {}",
        client_stdout
    );

    // Kill the server
    let kill_result = Command::new("kill")
        .args(&["-INT", &server.id().to_string()])
        .output()
        .unwrap();
    assert!(kill_result.status.success(), "kill server failed");
    let server_output = server.wait_with_output().unwrap();

    // Verify server did NOT print the data to its own stdout (it should only
    // broadcast to clients, not print locally)
    let server_stdout = String::from_utf8(server_output.stdout).unwrap();
    assert!(
        !server_stdout.contains("server_mode_test_line"),
        "server should NOT print data to stdout in server mode: {}",
        server_stdout
    );

    // Verify the fifo was cleaned up
    assert!(
        !std::path::Path::new(&fifo_path).exists(),
        "fifo should be removed after server exits"
    );
}

/// Server with grep: server runs with -g filter, client only receives matching lines.
/// This tests that the ServerWrite op is used in the grep path (not PrintPlain/PrintColor).
#[test]
fn test_server_grep_client() {
    let fifo_path = format!("/tmp/rog_test_server_grep_{}", std::process::id());
    let port = 19902;
    let _ = std::fs::remove_file(&fifo_path);

    // Start server with grep filter
    let mut server = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-s", "-f", &fifo_path, "-c", "-g", "ERROR", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Start client
    let client = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Write mixed data — only ERROR lines should reach the client
    let test_data = "INFO: all good\nERROR: something broke\nINFO: still going\nERROR: another fail\n";
    let mut fifo_writer = std::fs::File::create(&fifo_path).unwrap();
    fifo_writer.write_all(test_data.as_bytes()).unwrap();
    drop(fifo_writer);

    thread::sleep(Duration::from_millis(500));

    // Kill client
    let _ = Command::new("kill")
        .args(&["-INT", &client.id().to_string()])
        .output()
        .unwrap();
    let client_output = client.wait_with_output().unwrap();

    let client_stdout = String::from_utf8(client_output.stdout).unwrap();

    // Should contain ERROR lines
    assert!(
        client_stdout.contains("ERROR: something broke"),
        "client should receive ERROR lines: {}",
        client_stdout
    );
    assert!(
        client_stdout.contains("ERROR: another fail"),
        "client should receive ERROR lines: {}",
        client_stdout
    );

    // Should NOT contain INFO lines
    assert!(
        !client_stdout.contains("INFO"),
        "client should NOT receive INFO lines (grep filter): {}",
        client_stdout
    );

    // Kill server and cleanup
    let _ = Command::new("kill")
        .args(&["-INT", &server.id().to_string()])
        .output()
        .unwrap();
    server.wait().unwrap();
    let _ = std::fs::remove_file(&fifo_path);
}

/// Multiple clients: two clients connect to the same server, both receive the data.
#[test]
fn test_server_multiple_clients() {
    let fifo_path = format!("/tmp/rog_test_multi_{}", std::process::id());
    let port = 19903;
    let _ = std::fs::remove_file(&fifo_path);

    // Start server
    let mut server = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-s", "-f", &fifo_path, "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Start client 1
    let client1 = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Start client 2
    let client2 = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Write data to fifo
    let test_data = "broadcast_test_data\n";
    let mut fifo_writer = std::fs::File::create(&fifo_path).unwrap();
    fifo_writer.write_all(test_data.as_bytes()).unwrap();
    drop(fifo_writer);

    thread::sleep(Duration::from_millis(500));

    // Kill both clients
    let _ = Command::new("kill")
        .args(&["-INT", &client1.id().to_string()])
        .output()
        .unwrap();
    let _ = Command::new("kill")
        .args(&["-INT", &client2.id().to_string()])
        .output()
        .unwrap();

    let out1 = client1.wait_with_output().unwrap();
    let out2 = client2.wait_with_output().unwrap();

    let stdout1 = String::from_utf8(out1.stdout).unwrap();
    let stdout2 = String::from_utf8(out2.stdout).unwrap();

    // Both clients should receive the data
    assert!(
        stdout1.contains("broadcast_test_data"),
        "client1 should receive broadcast data: {}",
        stdout1
    );
    assert!(
        stdout2.contains("broadcast_test_data"),
        "client2 should receive broadcast data: {}",
        stdout2
    );

    // Kill server and cleanup
    let _ = Command::new("kill")
        .args(&["-INT", &server.id().to_string()])
        .output()
        .unwrap();
    server.wait().unwrap();
    let _ = std::fs::remove_file(&fifo_path);
}

/// Client fails to connect when no server is running on the target host.
#[test]
fn test_client_no_server() {
    let port = 19904; // unused port — no server listening
    let output = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
        .wait_with_output()
        .unwrap();

    // Should exit with error because no server is listening
    assert!(
        !output.status.success(),
        "client should fail when no server is running"
    );
}

/// File tailing: create 2 files, start rog watching them, append data, verify output.
/// This tests vm_tail end-to-end: file watching, reading new content, and multi-file handling.
#[test]
fn test_tail_two_files() {
    let id = std::process::id();
    let file1 = format!("/tmp/rog_test_file1_{}", id);
    let file2 = format!("/tmp/rog_test_file2_{}", id);

    // Ensure stale files are gone
    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);

    // Create empty files so rog can open and watch them
    std::fs::write(&file1, "").unwrap();
    std::fs::write(&file2, "").unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-c", &file1, &file2])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // Give rog time to set up file watches
    thread::sleep(Duration::from_millis(500));

    // Append data to file1
    let data1 = "line from file1\n";
    let mut f1 = std::fs::OpenOptions::new()
        .append(true)
        .open(&file1)
        .unwrap();
    f1.write_all(data1.as_bytes()).unwrap();
    drop(f1);

    // Append data to file2
    let data2 = "line from file2\n";
    let mut f2 = std::fs::OpenOptions::new()
        .append(true)
        .open(&file2)
        .unwrap();
    f2.write_all(data2.as_bytes()).unwrap();
    drop(f2);

    // Give rog time to detect modifications and read the data
    thread::sleep(Duration::from_millis(500));

    // Kill the child (rx.recv() blocks, preventing clean exit on SIGINT)
    let kill_result = Command::new("kill")
        .args(&["-9", &child.id().to_string()])
        .output()
        .unwrap();
    assert!(kill_result.status.success(), "kill -9 failed");

    let output = child.wait_with_output().unwrap();

    // Verify output contains data from both files
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("line from file1"), "output missing 'line from file1': {}", stdout);
    assert!(stdout.contains("line from file2"), "output missing 'line from file2': {}", stdout);

    // Clean up temp files
    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
}

/// Remove a single simple field (key=value).
#[test]
fn test_remove_single_field() {
    let input = r#"host=127.0.0.1 Starting up
host=127.0.0.1 Just a warning"#;
    let expected = r#"Starting up
Just a warning"#;
    run_test_stdin(&["-c", "-r", "host"], input, expected);
}

/// Remove multiple fields in one pass.
#[test]
fn test_remove_multiple_fields() {
    let input = r#"host=127.0.0.1 level=warn Just a warning
host=127.0.0.1 level=error An error occurred"#;
    let expected = r#"Just a warning
An error occurred"#;
    run_test_stdin(&["-c", "-r", "host,level"], input, expected);
}

/// Remove a field with a quoted value containing spaces.
#[test]
fn test_remove_quoted_field() {
    let input = r#"msg="hello world" some extra text"#;
    let expected = "some extra text";
    run_test_stdin(&["-c", "-r", "msg"], input, expected);
}

/// Remove mixed: one simple field and one quoted field.
#[test]
fn test_remove_mixed_fields() {
    let input = r#"status=ok msg="value with spaces" trailing text"#;
    let expected = "trailing text";
    run_test_stdin(&["-c", "-r", "status,msg"], input, expected);
}

/// When the field to remove is not present, the line should pass through unchanged.
#[test]
fn test_remove_no_match() {
    let input = r#"hello world
another line
still another"#;
    let expected = input;
    run_test_stdin(&["-c", "-r", "nonexistent"], input, expected);
}

/// Remove field and truncate the resulting line.
#[test]
fn test_remove_and_truncate() {
    let input = "key=val this is a very long line that should be truncated properly\n";
    let expected = "this is a very long line tha\n";
    run_test_stdin(&["-c", "-r", "key", "-o", "28"], input, expected);
}

/// Remove field on a line that gets truncated to the width limit.
#[test]
fn test_remove_and_truncate_short() {
    let input = "id=12345 hello\n";
    let expected = "he\n";
    run_test_stdin(&["-c", "-r", "id", "-o", "2"], input, expected);
}

/// Truncate alone (no remove) still works.
#[test]
fn test_truncate_alone() {
    let input = "hello world this is long\n";
    let expected = "he\n";
    run_test_stdin(&["-c", "-o", "2"], input, expected);
}

/// Remove with grep: only matching lines are printed and fields are removed.
#[test]
fn test_remove_with_grep() {
    let input = r#"key=val this line matches
other line no match
key=val another matches"#;
    let expected = r#"this line matches
another matches"#;
    run_test_stdin(&["-c", "-g", "matches", "-r", "key"], input, expected);
}

/// Remove with vgrep: filter out lines, then remove fields from remaining.
#[test]
fn test_remove_with_vgrep() {
    let input = r#"key=val keep this
key=val skip this please
keep this too no field"#;
    let expected = r#"keep this
keep this too no field"#;
    run_test_stdin(&["-c", "-v", "please", "-r", "key"], input, expected);
}

/// Remove field that appears multiple times on the same line.
#[test]
fn test_remove_field_repeated() {
    let input = r#"tag=a tag=b tag=c end of line"#;
    let expected = "end of line";
    run_test_stdin(&["-c", "-r", "tag"], input, expected);
}

/// Remove at start of line: field is the only content.
#[test]
fn test_remove_only_content() {
    let input = r#"key=val
other line
key=val"#;
    let expected = "\nother line\n";
    run_test_stdin(&["-c", "-r", "key"], input, expected);
}

/// Remove with back-to-back fields (no space between them).
#[test]
fn test_remove_adjacent_fields() {
    let input = r#"a=1b=2 after"#;
    let expected = "after";
    run_test_stdin(&["-c", "-r", "a,b"], input, expected);
}

/// Remove field at end of line (trailing space after value is also removed).
#[test]
fn test_remove_trailing_field() {
    let input = r#"start of line key=val"#;
    let expected = "start of line";
    run_test_stdin(&["-c", "-r", "key"], input, expected);
}

/// Remove with grep and context: fields removed from all printed lines.
#[test]
fn test_remove_with_grep_context() {
    let input = r#"key=val line one
key=val MATCH here
key=val line three"#;
    let expected = r#"line one
MATCH here
line three"#;
    run_test_stdin(&["-c", "-g", "MATCH", "-B1", "-A1", "-r", "key"], input, expected);
}
