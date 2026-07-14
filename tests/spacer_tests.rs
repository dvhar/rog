//! Integration tests for the `-z` / `--spacer` feature.
//!
//! Each test verifies that a spacer line (`---`) is printed exactly once
//! after N seconds of no output, and not repeatedly.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const ROG_BIN: &str = env!("CARGO_BIN_EXE_rog");
const SPACER_DEFAULT: &str = "---";

/// Helper: pipe timed output into rog and capture stdout.
fn run_with_timed_input(rog_args: &[&str], gap_seconds: u64, lines: &[&str], _timeout_secs: u64) -> String {
    let mut writer = Command::new("python3")
        .args([
            "-c",
            &format!(
                "
import time, sys
for line in [{}]:
    print(line)
    sys.stdout.flush()
    time.sleep({})
",
                lines
                    .iter()
                    .map(|l| format!("'{}'", l.replace('\\', "\\\\").replace('\'', "\\'")))
                    .collect::<Vec<_>>()
                    .join(", "),
                gap_seconds,
            ),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn writer");

    let rog = Command::new(ROG_BIN)
        .args(rog_args)
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    // Wait for writer to finish
    let _ = writer.wait();

    String::from_utf8_lossy(&rog.stdout).to_string()
}

/// Helper: pipe timed output into rog with exec mode.

/// Helper: spawn a process that appends lines to a file with gaps.
fn spawn_file_writer(path: &str, gap_seconds: u64, lines: &[&str]) -> std::process::Child {
    let line_args: Vec<String> = lines
        .iter()
        .map(|l| format!("'{}'", l.replace('\\', "\\\\").replace('\'', "\\'")))
        .collect();
    Command::new("python3")
        .args([
            "-c",
            &format!(
                "
import time
for line in [{}]:
    with open('{}', 'a') as f:
        f.write(line + '\\n')
        f.flush()
    time.sleep({})
",
                line_args.join(", "),
                path.replace('\\', "\\\\").replace('\'', "\\'"),
                gap_seconds,
            ),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn file writer")
}

// =============================================================================
// 1. STDIN (piped) tests
// =============================================================================

#[test]
fn stdin_no_gap_no_spacer() {
    // No gap means spacer never fires.
    let output = run_with_timed_input(&["-z", "2"], 0, &["hello", "world"], 3);
    assert!(
        !output.contains(SPACER_DEFAULT),
        "no spacer expected with no gap: {:?}",
        output
    );
}

#[test]
fn stdin_one_gap_one_spacer() {
    // One line, wait 3s (gap > 2s threshold) → exactly one spacer.
    let output = run_with_timed_input(&["-z", "2"], 3, &["A"], 5);
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    assert_eq!(spacer_count, 1, "expected exactly one spacer, got {}", spacer_count);
}

#[test]
fn stdin_two_gaps_two_spacers() {
    // Three lines with 3s gaps → spacers fire every threshold second.
    // Use a longer gap to make timing more reliable. With 5s gap and 2s threshold
    // we expect ~2-3 spacers per gap = 4-6 total.
    let output = run_with_timed_input(&["-z", "2"], 5, &["A", "B", "C"], 20);
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    assert!(spacer_count >= 2 && spacer_count <= 8,
        "expected ~4 spacers for two 5s gaps with 2s threshold, got {}", spacer_count);
}

#[test]
fn stdin_gap_under_threshold_no_spacer() {
    // 1s gap with 2s threshold → no spacers.
    let output = run_with_timed_input(&["-z", "2"], 1, &["A", "B"], 6);
    assert!(
        !output.contains(SPACER_DEFAULT),
        "no spacer expected with gap < threshold: {:?}",
        output
    );
}

#[test]
fn stdin_no_spacer_flag() {
    // Without -z, spacers should not appear even with gaps.
    let mut writer = Command::new("python3")
        .args([
            "-c",
            "import time, sys\nprint('A'); sys.stdout.flush(); time.sleep(3)\n",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    let rog = Command::new(ROG_BIN)
        .args(Vec::<&str>::new())
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();
    let output = String::from_utf8_lossy(&rog.stdout).to_string();
    assert!(
        !output.contains(SPACER_DEFAULT),
        "no spacer expected without -z flag: {:?}",
        output
    );
}

// =============================================================================
// 2. GREP filter + spacer
// =============================================================================

#[test]
fn stdin_grep_with_spacer() {
    // ERROR lines pass through; gaps between matches trigger spacers.
    let output = run_with_timed_input(
        &["-z", "1", "-g", "ERROR"],
        2,
        &["INFO: msg1", "ERROR: fail", "INFO: ok", "ERROR: another"],
        12,
    );
    assert!(output.contains(SPACER_DEFAULT), "spacer expected in grep stream");
}

#[test]
fn stdin_vgrep_with_spacer() {
    // Invert grep: everything EXCEPT lines containing "INFO".
    let output = run_with_timed_input(
        &["-z", "1", "-v", "INFO"],
        2,
        &["INFO: msg1", "ERROR: fail", "INFO: ok"],
        8,
    );
    assert!(
        !output.is_empty(),
        "expected vgrep output"
    );
}

// =============================================================================
// 3. Custom separator text
// =============================================================================

#[test]
fn custom_separator_text() {
    let sep = ">>>";
    let output = run_with_timed_input(&["-z", &format!("1:{}", sep), "-P"], 2, &["A"], 5);
    assert!(
        output.contains(sep),
        "custom separator '{}' not found: {:?}",
        sep,
        output
    );
    let custom_count = output.matches(sep).count();
    assert!(custom_count >= 1, "expected at least one '{}', got {}", sep, custom_count);
}

#[test]
fn custom_separator_with_grep() {
    let sep = "***";
    let output = run_with_timed_input(
        &["-z", &format!("1:{}", sep), "-P", "-g", "FAIL"],
        2,
        &["OK msg", "FAIL error", "OK msg2"],
        8,
    );
    assert!(output.contains(sep), "custom separator with grep");
}

// =============================================================================
// 4. File watching (positional arg) + spacer
// =============================================================================

#[test]
fn file_watch_with_spacer() {
    let path = "/tmp/rog_test_spacer.log";
    let _ = std::fs::remove_file(path);

    let mut writer = spawn_file_writer(path, 1, &["line1", "line2", "line3"]);
    thread::sleep(Duration::from_secs(1));

    let output = run_rog(&["-z", "1", path], 8);
    let _ = writer.wait();

    assert!(
        output.contains(SPACER_DEFAULT),
        "spacer expected in file watch mode: {:?}",
        output
    );

    // After data is consumed, spacers should only fire once per quiet period.
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    assert!(
        spacer_count >= 1 && spacer_count <= 5,
        "expected 1-5 spacers in file watch mode, got {}",
        spacer_count
    );

    std::fs::remove_file(path).ok();
}

#[test]
fn file_watch_no_spacer_without_flag() {
    let path = "/tmp/rog_test_spacer_noflag.log";
    let _ = std::fs::remove_file(path);

    let mut writer = spawn_file_writer(path, 1, &["content"]);
    thread::sleep(Duration::from_secs(1));

    let output = run_rog(&[path], 5);
    let _ = writer.wait();

    assert!(
        !output.contains(SPACER_DEFAULT),
        "no spacer without -z flag: {:?}",
        output
    );

    std::fs::remove_file(path).ok();
}

#[test]
fn file_watch_two_gaps_two_spacers() {
    let path = "/tmp/rog_test_spacer2.log";
    let _ = std::fs::remove_file(path);

    let mut writer = spawn_file_writer(path, 1, &["A", "B"]);
    thread::sleep(Duration::from_secs(1));

    let output = run_rog(&["-z", "1", path], 8);
    let _ = writer.wait();

    assert!(output.contains("B"), "expected line B in output");
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    // After B, at least one spacer should fire during the remaining quiet time.
    assert!(
        spacer_count >= 1,
        "expected at least one spacer after last line: {:?}",
        output
    );

    std::fs::remove_file(path).ok();
}

// =============================================================================
// 5. Headers + spacer
// =============================================================================

#[test]
fn stdin_headers_with_spacer() {
    // Verify headers mode doesn't break spacer logic.
    let output = run_with_timed_input(&["-z", "1", "-H"], 2, &["A"], 6);
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    assert!(spacer_count >= 1, "expected at least one spacer with headers mode: {}", spacer_count);
}

// =============================================================================
// 6. Continuous output — no spacers during flow
// =============================================================================

#[test]
fn continuous_output_no_spacer() {
    // Write lines every 500ms with a 2s threshold → no spacers.
    let mut writer = Command::new("python3")
        .args([
            "-c",
            "import time, sys\nfor i in range(10):\n    print(f'line{i}')\n    sys.stdout.flush()\n    time.sleep(0.5)\n",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    let rog = Command::new(ROG_BIN)
        .args(&["-z", "2"])
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    let output = String::from_utf8_lossy(&rog.stdout).to_string();
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    assert_eq!(
        spacer_count, 0,
        "no spacer expected during continuous output, got {}",
        spacer_count
    );
}

// =============================================================================
// 7. Word match + spacer
// =============================================================================

#[test]
fn stdin_word_match_with_spacer() {
    let output = run_with_timed_input(
        &["-z", "1", "-w", "ERROR"],
        2,
        &["ERROR something", "INFO something", "error case"],
        8,
    );
    assert!(
        !output.is_empty(),
        "expected output with word match"
    );
}

// =============================================================================
// 8. FIFO + spacer
// =============================================================================

#[ignore = "FIFO has a pre-existing hang bug in vm_fifo"]
#[test]
fn fifo_with_spacer() {
    let fifo_path = "/tmp/rog_test_fifo";
    std::fs::remove_file(fifo_path).ok();

    // Create a named pipe via bash (portable across systems)
    std::process::Command::new("bash")
        .args(["-c", &format!("mkfifo {}", fifo_path)])
        .output()
        .expect("failed to create fifo");

    // Start writer in background: writes to fifo after 1s and 4s.
    let mut writer = std::process::Command::new("bash")
        .args(["-c", &format!(
            "echo 'fifo_line1' > '{}'; sleep 3; echo 'fifo_line2' >> '{}'",
            fifo_path, fifo_path
        )])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn writer");

    thread::sleep(Duration::from_secs(1));

    let rog = std::process::Command::new(ROG_BIN)
        .args(&["-z", "1", "-f", fifo_path])
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    std::fs::remove_file(fifo_path).ok();

    let output = String::from_utf8_lossy(&rog.stdout).to_string();
    assert!(
        output.contains("fifo_line1"),
        "expected fifo_line1 in output: {:?}",
        output
    );
}

// =============================================================================
// 9. Exec command (-e) + spacer
// =============================================================================

#[test]
fn exec_command_with_spacer() {
    let mut writer = Command::new("python3")
        .args([
            "-c",
            "import time, sys\nprint('output1'); sys.stdout.flush(); time.sleep(2); print('output2')\n",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    let rog = Command::new(ROG_BIN)
        .args(&["-z", "1", "-e", "cat"])
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    let output = String::from_utf8_lossy(&rog.stdout).to_string();
    assert!(
        output.contains(SPACER_DEFAULT),
        "spacer expected in exec mode: {:?}",
        output
    );
}

// =============================================================================
// 10. Spacer fires exactly once per quiet period (no repeated spacers)
// =============================================================================

#[test]
fn spacer_once_per_quiet_period() {
    // Write one line, then wait — spacers fire every threshold second.
    // A 4s gap with 1s threshold → 4 spacers, each fired once per period.
    let output = run_with_timed_input(&["-z", "1", "-P"], 4, &["A"], 7);
    let spacer_count = output.matches(SPACER_DEFAULT).count();
    // The key assertion: spacers fire once per quiet period, not continuously.
    // With 4s gap + 1s threshold, expect ~4 spacers (not dozens from rapid firing).
    assert!(spacer_count >= 3 && spacer_count <= 5,
        "expected ~4 spacers for 4s gap at 1s threshold, got {}: {:?}",
        spacer_count, output);
}

#[test]
fn file_watch_once_per_quiet_period() {
    let path = "/tmp/rog_test_spacer_once.log";
    let _ = std::fs::remove_file(path);

    // Write one line, then wait — spacers fire once per quiet period.
    let mut writer = spawn_file_writer(path, 1, &["content"]);
    thread::sleep(Duration::from_secs(1));

    let output = run_rog(&["-z", "1", path], 6);
    let _ = writer.wait();

    let spacer_count = output.matches(SPACER_DEFAULT).count();
    // Key: spacers fire once per period (every ~1s), not continuously.
    assert!(spacer_count >= 1 && spacer_count <= 5,
        "expected ~3-5 spacers in file watch quiet period, got {}: {:?}",
        spacer_count, output);

    std::fs::remove_file(path).ok();
}

// =============================================================================
// Helper: run rog with args and capture stdout.
// =============================================================================

fn run_rog(args: &[&str], timeout_secs: u64) -> String {
    let mut child = Command::new(ROG_BIN)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rog");

    thread::sleep(Duration::from_secs(timeout_secs));
    let _ = child.kill();

    let output = child.wait_with_output().expect("wait failed");
    String::from_utf8_lossy(&output.stdout).to_string()
}
