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

/// Headers: when tailing multiple files, a header line `==> path <==` is printed
/// when switching from one file to another.
#[test]
fn test_tail_multiple_files_shows_headers() {
    let id = std::process::id();
    let file1 = format!("/tmp/rog_header_test1_{}", id);
    let file2 = format!("/tmp/rog_header_test2_{}", id);

    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
    std::fs::write(&file1, "").unwrap();
    std::fs::write(&file2, "").unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-c", &file1, &file2])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Write to file1
    let mut f1 = std::fs::OpenOptions::new().append(true).open(&file1).unwrap();
    f1.write_all(b"line from file1\n").unwrap();
    drop(f1);

    // Small delay to ensure rog processes file1 event first
    thread::sleep(Duration::from_millis(300));

    // Write to file2
    let mut f2 = std::fs::OpenOptions::new().append(true).open(&file2).unwrap();
    f2.write_all(b"line from file2\n").unwrap();
    drop(f2);

    thread::sleep(Duration::from_millis(500));

    let _ = Command::new("kill")
        .args(&["-9", &child.id().to_string()])
        .output()
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Both lines should be present
    assert!(stdout.contains("line from file1"), "missing file1 data: {}", stdout);
    assert!(stdout.contains("line from file2"), "missing file2 data: {}", stdout);

    // Header should appear in output when tailing multiple files
    assert!(
        stdout.contains("==>"),
        "header marker '==>' should appear when tailing multiple files: {}",
        stdout
    );
    assert!(
        stdout.contains("<=="),
        "header marker '<==' should appear when tailing multiple files: {}",
        stdout
    );

    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
}

/// Headers: when tailing a single file, no header lines should appear.
#[test]
fn test_tail_single_file_no_headers() {
    let id = std::process::id();
    let file1 = format!("/tmp/rog_single_header_{}", id);

    let _ = std::fs::remove_file(&file1);
    std::fs::write(&file1, "").unwrap();

    let child = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-c", &file1])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    let data = "line from single file\n";
    let mut f1 = std::fs::OpenOptions::new().append(true).open(&file1).unwrap();
    f1.write_all(data.as_bytes()).unwrap();
    drop(f1);

    thread::sleep(Duration::from_millis(500));

    let _ = Command::new("kill")
        .args(&["-9", &child.id().to_string()])
        .output()
        .unwrap();

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Data should be present
    assert!(
        stdout.contains("line from single file"),
        "missing data: {}",
        stdout
    );

    // No headers should appear for a single file
    assert!(
        !stdout.contains("==>"),
        "header marker '==>' should NOT appear when tailing a single file: {}",
        stdout
    );
    assert!(
        !stdout.contains("<=="),
        "header marker '<==' should NOT appear when tailing a single file: {}",
        stdout
    );

    let _ = std::fs::remove_file(&file1);
}

/// Server with multiple files: when a server tails multiple files, headers are
/// sent to connected clients when switching between files.
#[test]
fn test_server_multi_files_headers_to_client() {
    let id = std::process::id();
    let file1 = format!("/tmp/rog_svr_header1_{}", id);
    let file2 = format!("/tmp/rog_svr_header2_{}", id);
    let port = 19905;

    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
    std::fs::write(&file1, "").unwrap();
    std::fs::write(&file2, "").unwrap();

    // Start server in multi-file mode (no fifo, just tailing files)
    let mut server = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-s", "-c", "-d", &port.to_string(), &file1, &file2])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Start client connected to server
    let client = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-k", "-c", "-d", &port.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    // Write to file1
    let mut f1 = std::fs::OpenOptions::new().append(true).open(&file1).unwrap();
    f1.write_all(b"svr line from file1\n").unwrap();
    drop(f1);

    thread::sleep(Duration::from_millis(300));

    // Write to file2 — this should trigger a header before the data
    let mut f2 = std::fs::OpenOptions::new().append(true).open(&file2).unwrap();
    f2.write_all(b"svr line from file2\n").unwrap();
    drop(f2);

    thread::sleep(Duration::from_millis(500));

    // Kill client
    let _ = Command::new("kill")
        .args(&["-INT", &client.id().to_string()])
        .output()
        .unwrap();
    let client_output = client.wait_with_output().unwrap();

    let client_stdout = String::from_utf8(client_output.stdout).unwrap();

    // Client should receive data from both files
    assert!(
        client_stdout.contains("svr line from file1"),
        "client missing file1 data: {}",
        client_stdout
    );
    assert!(
        client_stdout.contains("svr line from file2"),
        "client missing file2 data: {}",
        client_stdout
    );

    // Client should receive headers (server sends them to clients)
    assert!(
        client_stdout.contains("==>"),
        "client should receive header markers when server tails multiple files: {}",
        client_stdout
    );
    assert!(
        client_stdout.contains("<=="),
        "client should receive header markers when server tails multiple files: {}",
        client_stdout
    );

    // Kill server and cleanup
    let _ = Command::new("kill")
        .args(&["-9", &server.id().to_string()])
        .output()
        .unwrap();
    let _ = server.wait();
    let _ = std::fs::remove_file(&file1);
    let _ = std::fs::remove_file(&file2);
}

/// Client mode reading from socket: client itself does not generate headers
/// even when connected to a server. The client merely processes the stream it
/// receives (headers are generated by the server, not the client).
/// This test uses a single-file server so the server sends no headers,
/// verifying the client does not fabricate its own.
#[test]
fn test_client_no_headers_single_file_server() {
    let id = std::process::id();
    let fifo_path = format!("/tmp/rog_client_hdr_{}", id);
    let port = 19906;
    let _ = std::fs::remove_file(&fifo_path);

    // Start server with single fifo (no headers in server mode either)
    let mut server = Command::new(env!("CARGO_BIN_EXE_rog"))
        .args(&["-s", "-f", &fifo_path, "-c", "-d", &port.to_string()])
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

    // Write data to fifo
    let test_data = "client_no_header_test\n";
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

    // Data should be present
    assert!(
        client_stdout.contains("client_no_header_test"),
        "client missing data: {}",
        client_stdout
    );

    // Client should NOT have any headers (server is single-source, client never generates headers)
    assert!(
        !client_stdout.contains("==>"),
        "client should NOT show headers when server has single source: {}",
        client_stdout
    );

    // Kill server
    let _ = Command::new("kill")
        .args(&["-INT", &server.id().to_string()])
        .output()
        .unwrap();
    let _ = server.wait();
    let _ = std::fs::remove_file(&fifo_path);
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

/// Parse file headers from stdin: -H flag parses ==>/<== markers and assigns
/// different colors to each file. Without -c, output should contain ANSI codes.
#[test]
fn test_parse_headers_from_stdin() {
    let input = r#"==> /var/log/syslog <==
Jan  1 normal log line
==> /var/log/auth.log <==
Jan  2 auth event here
==> /var/log/syslog <==
Jan  3 another syslog line"#;

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rog"));
    cmd.args(&["-H"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let stdin = child.stdin.as_mut().unwrap();
    stdin.write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Headers should be present in output
    assert!(
        stdout.contains("==> /var/log/syslog <=="),
        "missing syslog header: {}",
        stdout
    );
    assert!(
        stdout.contains("==> /var/log/auth.log <=="),
        "missing auth.log header: {}",
        stdout
    );

    // Log lines should be present
    assert!(
        stdout.contains("normal log line"),
        "missing syslog log line: {}",
        stdout
    );
    assert!(
        stdout.contains("auth event here"),
        "missing auth.log log line: {}",
        stdout
    );
    assert!(
        stdout.contains("another syslog line"),
        "missing second syslog line: {}",
        stdout
    );

    // Without -c flag, output should contain ANSI color codes for syntax highlighting
    assert!(
        stdout.contains("\x1b["),
        "output should contain ANSI escape codes (no -c flag): {}",
        stdout
    );

    // The two files should get different color codes (different ANSI sequences)
    let syslog_line = stdout
        .lines()
        .find(|l| l.contains("normal log line"))
        .expect("syslog line not found");
    let auth_line = stdout
        .lines()
        .find(|l| l.contains("auth event here"))
        .expect("auth line not found");

    // Extract ANSI codes from each line
    let syslog_ansi: Vec<&str> = syslog_line.matches("\x1b[").collect();
    let auth_ansi: Vec<&str> = auth_line.matches("\x1b[").collect();

    // Both lines should have ANSI codes
    assert!(!syslog_ansi.is_empty(), "syslog line should have ANSI codes");
    assert!(!auth_ansi.is_empty(), "auth line should have ANSI codes");

    // The ANSI codes should differ between files (different colors)
    let syslog_codes = syslog_line
        .split('\x1b')
        .skip(1)
        .next()
        .unwrap_or("");
    let auth_codes = auth_line
        .split('\x1b')
        .skip(1)
        .next()
        .unwrap_or("");
    assert!(
        syslog_codes != auth_codes,
        "syslog and auth lines should have different ANSI color codes\nsyslog: {}\nauth: {}",
        syslog_codes,
        auth_codes
    );
}

// ─── Start/Stop Pattern Tests (-a and -b) ──────────────────────────────────

/// -a only: skip lines until start pattern matches (inclusive), then print rest.
#[test]
fn test_start_pattern_only() {
    let input = "skip this\nalso skip\nSTART here\nprint this\nand this\n";
    let expected = "START here\nprint this\nand this";
    run_test_stdin(&["-c", "-a", "START"], input, expected);
}

/// -a with no match: nothing is printed.
#[test]
fn test_start_pattern_no_match() {
    let input = "line one\nline two\nline three\n";
    run_test_stdin(&["-c", "-a", "NONEXISTENT"], input, "");
}

/// -a on first line: everything prints (start matches immediately).
#[test]
fn test_start_pattern_first_line() {
    let input = "START now\nsecond line\nthird line\n";
    let expected = "START now\nsecond line\nthird line";
    run_test_stdin(&["-c", "-a", "START"], input, expected);
}

/// -b only: print everything until stop pattern matches (inclusive), then exit.
#[test]
fn test_stop_pattern_only() {
    let input = "print this\nand this\nSTOP here\nshould not see\n";
    let expected = "print this\nand this\nSTOP here";
    run_test_stdin(&["-c", "-b", "STOP"], input, expected);
}

/// -b with no match: everything prints (nothing triggers exit).
#[test]
fn test_stop_pattern_no_match() {
    let input = "line one\nline two\nline three\n";
    let expected = "line one\nline two\nline three";
    run_test_stdin(&["-c", "-b", "NONEXISTENT"], input, expected);
}

/// -b on first line: only first line is printed, then exit.
#[test]
fn test_stop_pattern_first_line() {
    let input = "STOP immediately\nshould not see\nneither this\n";
    let expected = "STOP immediately";
    run_test_stdin(&["-c", "-b", "STOP"], input, expected);
}

/// -a and -b together: wait for start, print until stop (both trigger lines included).
#[test]
fn test_start_and_stop_patterns() {
    let input = "skip\nSTART here\nkeep this\nSTOP here\nskip again\n";
    let expected = "START here\nkeep this\nSTOP here";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP"], input, expected);
}

/// -a and -b with multiple cycles: prints each start-stop block.
#[test]
fn test_start_and_stop_multiple_cycles() {
    let input = "skip\nSTART\nkeep1\nSTOP\nskip\nSTART\nkeep2\nSTOP\nskip\n";
    let expected = "START\nkeep1\nSTOP\nSTART\nkeep2\nSTOP";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP"], input, expected);
}

/// -a and -b where stop appears before start: prints nothing.
#[test]
fn test_stop_before_start() {
    let input = "STOP here\nSTART here\ndata\n";
    let expected = "START here\ndata";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP"], input, expected);
}

/// -a and -b with neither pattern matching: prints nothing.
#[test]
fn test_start_and_stop_no_match() {
    let input = "line one\nline two\nline three\n";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP"], input, "");
}

/// -a with case-insensitive flag -i.
#[test]
fn test_start_pattern_case_insensitive() {
    let input = "skip\nstart here with lowercase\nkeep this\n";
    let expected = "start here with lowercase\nkeep this";
    run_test_stdin(&["-c", "-a", "START", "-i"], input, expected);
}

/// -b with case-insensitive flag -i.
#[test]
fn test_stop_pattern_case_insensitive() {
    let input = "keep this\nstop here lowercase\nshould not see\n";
    let expected = "keep this\nstop here lowercase";
    run_test_stdin(&["-c", "-b", "STOP", "-i"], input, expected);
}

/// -a and -b combined with grep -g: start/stop controls outer print window,
/// grep controls which lines within that window are shown.
#[test]
fn test_start_stop_with_grep() {
    let input = "skip\nSTART\nINFO: normal\nERROR: boom\nINFO: ok\nSTOP\nskip\n";
    let expected = "ERROR: boom";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-g", "ERROR"], input, expected);
}

/// -a and -b combined with vgrep -v: start/stop controls window, vgrep excludes lines.
/// START and STOP lines themselves are not filtered by vgrep.
#[test]
fn test_start_stop_with_vgrep() {
    let input = "skip\nSTART\nkeep this\nfilter this out\nkeep that\nSTOP\nskip\n";
    let expected = "START\nkeep this\nkeep that\nSTOP";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-v", "filter"], input, expected);
}

/// -a only, combined with width -o: truncated lines after start pattern.
#[test]
fn test_start_pattern_with_width() {
    let input = "skip this\nSTART\nthis is a very long line that exceeds the limit\n";
    let expected = "START\nthis is a very lon";
    run_test_stdin(&["-c", "-a", "START", "-o", "18"], input, expected);
}

/// -b only, combined with field removal -r.
#[test]
fn test_stop_pattern_with_remove() {
    let input = "key=val keep this\nkey=val keep that\nSTOP line\nkey=val skip\n";
    let expected = "keep this\nkeep that\nSTOP line";
    run_test_stdin(&["-c", "-b", "STOP", "-r", "key"], input, expected);
}

/// -a with no trailing newline on last line.
#[test]
fn test_start_pattern_no_trailing_newline() {
    let input = "skip\nSTART here\nlast line";
    let expected = "START here\nlast line";
    run_test_stdin(&["-c", "-a", "START"], input, expected);
}

/// -b with no trailing newline: stop line still printed, nothing after.
#[test]
fn test_stop_pattern_no_trailing_newline() {
    let input = "keep\nSTOP here\nafter stop";
    let expected = "keep\nSTOP here";
    run_test_stdin(&["-c", "-b", "STOP"], input, expected);
}

// ─── Lines (-l) After Stop Pattern Tests ──────────────────────────────────

/// -b with -l 1: stop line plus exactly 1 more line, then exit.
#[test]
fn test_stop_with_lines_one() {
    let input = "line1\nSTOP here\nextra one\nextra two\n";
    let expected = "line1\nSTOP here\nextra one";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "1"], input, expected);
}

/// -b with -l 2: stop line plus exactly 2 more lines, then exit.
#[test]
fn test_stop_with_lines_two() {
    let input = "line1\nSTOP here\nextra one\nextra two\nextra three\n";
    let expected = "line1\nSTOP here\nextra one\nextra two";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "2"], input, expected);
}

/// -b with -l 3: stop line plus exactly 3 more lines, then exit.
#[test]
fn test_stop_with_lines_three() {
    let input = "line1\nSTOP here\ne1\ne2\ne3\ne4\n";
    let expected = "line1\nSTOP here\ne1\ne2\ne3";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "3"], input, expected);
}

/// -b with -l N: the line immediately after the N extra lines is NOT printed.
#[test]
fn test_stop_with_lines_excludes_next() {
    let input = "keep\nSTOP here\nafter1\nafter2\nshould_not_appear\n";
    let expected = "keep\nSTOP here\nafter1\nafter2";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "2"], input, expected);
}

/// -b with -l 0 is equivalent to plain -b (no extra lines).
#[test]
fn test_stop_with_lines_zero() {
    let input = "print this\nSTOP here\nskip this\n";
    let expected = "print this\nSTOP here";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "0"], input, expected);
}

/// -b with -l N but fewer lines remain than N: prints what's available and exits.
#[test]
fn test_stop_with_lines_not_enough() {
    let input = "line1\nSTOP here\ne1\n";
    let expected = "line1\nSTOP here\ne1";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "5"], input, expected);
}

/// -b with -l N and no match: all lines print (no extra line handling needed).
#[test]
fn test_stop_with_lines_no_match() {
    let input = "line1\nline2\nline3\n";
    let expected = "line1\nline2\nline3";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -l but no -b: start pattern resumes printing after stop countdown.
#[test]
fn test_start_stop_with_lines_repeat_basic() {
    let input = "skip1\nSTART\nkeep1\nSTOP here\nafter1\nafter2\nskip2\nSTART\nkeep2\nSTOP here\nafter3\nafter4\nskip3\n";
    let expected = "START\nkeep1\nSTOP here\nafter1\nafter2\nSTART\nkeep2\nSTOP here\nafter3\nafter4";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -b with -l 1: exactly one line after each STOP.
#[test]
fn test_start_stop_with_lines_one_each() {
    let input = "A\nSTART B\nC\nSTOP D\nE\nF\nSTART G\nH\nSTOP I\nJ\nK\n";
    let expected = "START B\nC\nSTOP D\nE\nSTART G\nH\nSTOP I\nJ";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "1"], input, expected);
}

/// -a with -b with -l N: second stop during countdown resets the countdown.
#[test]
fn test_start_stop_with_lines_reset_countdown() {
    // After first STOP, await=3. Second STOP appears while awaiting -> resets to 3.
    // So we get 2 extra lines after second STOP instead of resuming at START.
    let input = "A\nSTART B\nC\nSTOP D\nE\nSTOP F\nG\nH\nSKIP I\n";
    let expected = "START B\nC\nSTOP D\nE\nSTOP F\nG\nH";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -b with -l N: no second start or stop during countdown — just extra lines then exit.
#[test]
fn test_start_stop_with_lines_no_resume() {
    let input = "skip\nSTART here\ndata line\nSTOP marker\nafter1\nafter2\nafter3\n";
    let expected = "START here\ndata line\nSTOP marker\nafter1\nafter2";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -b with -l N: stop appears before start — countdown is not active initially.
/// First START triggers printing. When STOP fires (await=0), countdown starts.
#[test]
fn test_start_stop_with_lines_stop_before_start() {
    let input = "STOP early\nSTART here\nkeep1\nkeep2\nSTOP end\nafter1\nafter2\nskip\n";
    let expected = "START here\nkeep1\nkeep2\nSTOP end\nafter1\nafter2";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -b with -l N: multiple cycles with exact counts.
#[test]
fn test_start_stop_with_lines_multiple_cycles() {
    let input =
        "A1\nS1 B1\nC1\nT1 D1\nE1\nA2\nS2 B2\nC2\nT2 D2\nE2\nA3\n";
    // S=START, T=STOP. -l 1 => 1 extra line after each STOP
    let expected = "S1 B1\nC1\nT1 D1\nE1\nS2 B2\nC2\nT2 D2\nE2";
    run_test_stdin(&["-c", "-a", "S", "-b", "T", "-l", "1"], input, expected);
}

/// -a with -b with -l 0: behaves like plain -a -b (no extra lines).
#[test]
fn test_start_stop_with_lines_zero_extra() {
    let input = "skip\nSTART keep STOP skip2 START keep2 STOP skip3\n";
    let expected = "START keep STOP START keep2 STOP";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "0"], input, expected);
}

/// -a with -b with -l N: extra lines can contain stop pattern without resetting.
/// (Only a stop while printing resets countdown; during countdown, stop resets it again.)
#[test]
fn test_stop_in_extra_lines_resets() {
    // STOP in the extra-2 window: since await > 0 and StopCheck fires, reset to 3
    let input = "A\nSTART B\nC\nSTOP D\nE\nSTOP F\nG\nH\nI\n";
    let expected = "START B\nC\nSTOP D\nE\nSTOP F\nG\nH";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -b with -l N on first line: stop line plus N extra lines.
#[test]
fn test_stop_with_lines_first_line() {
    let input = "STOP now\nafter1\nafter2\nafter3\n";
    let expected = "STOP now\nafter1\nafter2";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "2"], input, expected);
}

/// -a with -b with -l N: start appears on first line.
#[test]
fn test_start_stop_with_lines_first_line() {
    let input = "START B\nC\nSTOP D\nE\nF\n";
    let expected = "START B\nC\nSTOP D\nE\nF";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}

/// -b with -l N and grep: stop/lines controls outer window, grep filters inside.
#[test]
fn test_stop_with_lines_and_grep() {
    let input = "INFO ok\nERROR boom\nINFO more\nSTOP here\nINFO extra1\nERROR extra2\nSKIP me\n";
    let expected = "INFO ok\nERROR boom\nINFO more\nSTOP here\nINFO extra1\nERROR extra2";
    run_test_stdin(&["-c", "-b", "STOP", "-l", "2", "-g", "ERROR|INFO"], input, expected);
}

/// -a with -b with -l N and grep: combined filtering in repeat mode.
#[test]
fn test_start_stop_with_lines_and_grep() {
    let input = "skip\nSTART B\nINFO C\nERROR D\nSTOP E\nINFO F\nSKIP G\nSTART H\nINFO I\nERROR J\nSTOP K\n";
    let expected = "START B\nINFO C\nERROR D\nSTOP E\nINFO F\nSTART H\nINFO I\nERROR J\nSTOP K";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "1", "-g", "ERROR|INFO"], input, expected);
}

/// -a with -b with -l N and vgrep: exclude lines from output in repeat mode.
#[test]
fn test_start_stop_with_lines_and_vgrep() {
    let input = "skip\nSTART B\nkeep C\nfilter D\nSTOP E\nF\nG\nskip2\n";
    let expected = "START B\nkeep C\nSTOP E\nF";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "1", "-v", "filter"], input, expected);
}

/// -a with -b with -l N: no second start/stop after first countdown — exits cleanly.
#[test]
fn test_start_stop_with_lines_ends_without_resume() {
    let input = "pre\nSTART start\nmiddle\nSTOP end\nafter1\nafter2\nafter3 not shown\n";
    let expected = "START start\nmiddle\nSTOP end\nafter1\nafter2";
    run_test_stdin(&["-c", "-a", "START", "-b", "STOP", "-l", "2"], input, expected);
}
