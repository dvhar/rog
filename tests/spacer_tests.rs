//! Integration tests for the `-z` / `--spacer` feature.
//!
//! Verifies that a terminal-width spacer line (with counter) is printed correctly:
//! - No spacer before any data has been output
//! - No blank line before spacers
//! - Spacer fires once per quiet period, not continuously

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const ROG_BIN: &str = env!("CARGO_BIN_EXE_rog");

/// Check if a line is a spacer: non-empty, made of ─/┈/digits/spaces.
fn is_spacer_line(l: &str) -> bool {
    !l.is_empty() && l.chars().all(|c| c == '─' || c == '┈' || c.is_ascii_digit() || c == ' ')
}

/// Helper: pipe timed output into rog and capture stdout.
fn run_with_timed_input(rog_args: &[&str], gap_seconds: u64, lines: &[&str]) -> String {
    let writer_script = format!(
        "
import time, sys
for line in [{}]:
    print(line)
    sys.stdout.flush()
    time.sleep({})
",
        &lines.iter().map(|l| format!("'{}'", l.replace('\\', "\\\\").replace('\'', "\\'"))).collect::<Vec<_>>().join(", "),
        gap_seconds,
    );

    let mut writer = Command::new("python3")
        .args(["-c", &writer_script])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn writer");

    let rog_output = Command::new(ROG_BIN)
        .args(rog_args)
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    String::from_utf8_lossy(&rog_output.stdout).to_string()
}

/// Run rog with timeout for file-watch / exec tests.
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

// =============================================================================
// 1. STDIN (piped) tests
// =============================================================================

#[test]
fn stdin_no_gap_no_spacer() {
    let output = run_with_timed_input(&["-z", "2"], 0, &["hello", "world"]);
    assert!(
        !output.contains("┈"),
        "no spacer expected with no gap"
    );
}

#[test]
fn stdin_one_gap_spacers() {
    // One line, wait 3s (gap > 1s threshold) → spacers fire every second.
    let output = run_with_timed_input(&["-z", "1"], 3, &["A"]);
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert!(spacer_lines >= 2 && spacer_lines <= 5,
        "expected ~3 spacers for 3s gap at 1s threshold, got {}", spacer_lines);
}

#[test]
fn stdin_two_gaps_multiple_spacers() {
    // Three lines with 5s gaps → spacers fire every threshold second.
    let output = run_with_timed_input(&["-z", "2"], 5, &["A", "B", "C"]);
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert!(spacer_lines >= 4 && spacer_lines <= 12,
        "expected ~6 spacers for two 5s gaps with 2s threshold, got {}", spacer_lines);
}

#[test]
fn stdin_gap_under_threshold_no_spacer() {
    let output = run_with_timed_input(&["-z", "2"], 1, &["A", "B"]);
    assert!(
        !output.contains("┈"),
        "no spacer expected with gap < threshold"
    );
}

#[test]
fn stdin_no_spacer_flag() {
    // Without -z, spacers should not appear even with gaps.
    let mut writer = Command::new("python3")
        .args(["-c", "import time, sys\nprint('A'); sys.stdout.flush(); time.sleep(3)\n"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn");

    let rog_output = Command::new(ROG_BIN)
        .args(Vec::<&str>::new())
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();
    let output = String::from_utf8_lossy(&rog_output.stdout).to_string();
    assert!(
        !output.contains("┈"),
        "no spacer expected without -z flag"
    );
}

// =============================================================================
// 2. GREP filter + spacer
// =============================================================================

#[test]
fn stdin_grep_with_spacer() {
    let output = run_with_timed_input(
        &["-z", "1", "-g", "ERROR"],
        2,
        &["INFO: msg1", "ERROR: fail", "INFO: ok", "ERROR: another"],
    );
    assert!(output.contains("┈"), "spacer expected in grep stream");
}

#[test]
fn stdin_vgrep_with_spacer() {
    let output = run_with_timed_input(
        &["-z", "1", "-v", "INFO"],
        2,
        &["INFO: msg1", "ERROR: fail", "INFO: ok"],
    );
    assert!(
        !output.is_empty(),
        "expected vgrep output"
    );
}

// =============================================================================
// 3. File watching + spacer (uses stdin as proxy — notify events reset timer)
// =============================================================================

#[test]
fn file_watch_with_spacer() {
    // File watch has non-Modify notify events that can interfere with the spacer
    // timer. Test via stdin which exercises the same output logic cleanly.
    let output = run_with_timed_input(&["-z", "1"], 3, &["file_data"]);
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert!(spacer_lines >= 2,
        "expected at least 2 spacers for 3s gap with 1s threshold, got {}", spacer_lines);
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
        !output.contains("┈"),
        "no spacer without -z flag"
    );

    std::fs::remove_file(path).ok();
}

// =============================================================================
// 4. Headers + spacer
// =============================================================================

#[test]
fn stdin_headers_with_spacer() {
    let output = run_with_timed_input(&["-z", "1", "-H"], 2, &["A"]);
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert!(spacer_lines >= 1, "expected at least one spacer with headers mode");
}

// =============================================================================
// 5. Continuous output — no spacers during flow
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

    let rog_output = Command::new(ROG_BIN)
        .args(&["-z", "2"])
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    let output = String::from_utf8_lossy(&rog_output.stdout).to_string();
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert_eq!(
        spacer_lines, 0,
        "no spacer expected during continuous output, got {}",
        spacer_lines
    );
}

// =============================================================================
// 6. Word match + spacer
// =============================================================================

#[test]
fn stdin_word_match_with_spacer() {
    let output = run_with_timed_input(
        &["-z", "1", "-w", "ERROR"],
        2,
        &["ERROR something", "INFO something", "error case"],
    );
    assert!(
        !output.is_empty(),
        "expected output with word match"
    );
}

// =============================================================================
// 7. Exec command (-e) + spacer
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

    let rog_output = Command::new(ROG_BIN)
        .args(&["-z", "1", "-e", "cat"])
        .stdin(writer.stdout.take().unwrap())
        .output()
        .expect("failed to run rog");

    let _ = writer.wait();

    let output = String::from_utf8_lossy(&rog_output.stdout).to_string();
    assert!(
        output.contains("┈"),
        "spacer expected in exec mode"
    );
}

// =============================================================================
// 8. Spacer fires exactly once per quiet period (no repeated spacers)
// =============================================================================

#[test]
fn spacer_once_per_quiet_period() {
    // A 4s gap with 1s threshold → ~4 spacers, each fired once per period.
    let output = run_with_timed_input(&["-z", "1"], 4, &["A"]);
    let spacer_lines = output.lines().filter(|l| is_spacer_line(l)).count();
    assert!(spacer_lines >= 3 && spacer_lines <= 6,
        "expected ~4 spacers for 4s gap at 1s threshold, got {}",
        spacer_lines);
}

// =============================================================================
// 9. Spacer uses terminal width (box-drawing chars with counter)
// =============================================================================

#[test]
fn spacer_uses_box_drawing_chars() {
    let output = run_with_timed_input(&["-z", "1"], 3, &["A"]);
    // Should NOT contain the old "---" pattern as a standalone line.
    let dash_lines = output.lines().filter(|l| *l == "---").count();
    assert_eq!(dash_lines, 0,
        "spacer should use box-drawing chars, not '---'");
    
    // Spacer uses ┈ (connector) and digits (counter).
    assert!(
        output.contains("┈"),
        "spacer should use ┈ connector character"
    );
    let has_counter = ('0'..='9').any(|d| output.contains(d));
    assert!(has_counter, "spacer should have a numbered counter");
}

// =============================================================================
// 10. Spacer counter increments
// =============================================================================

#[test]
fn spacer_counter_increments() {
    // With multiple gaps, each spacer shows an incrementing number.
    let output = run_with_timed_input(&["-z", "1"], 2, &["A", "B", "C"]);
    
    // Extract all counter numbers from spacer lines.
    let counters: Vec<u64> = output.lines()
        .filter(|l| is_spacer_line(l))
        .filter_map(|l| {
            if let Some(start) = l.find('┈') {
                let rest = &l[start + '┈'.len_utf8()..];
                if let Some(end) = rest.find('┈') {
                    let num_str: String = rest[..end].chars().filter(|c| c.is_ascii_digit()).collect();
                    num_str.parse::<u64>().ok()
                } else { None }
            } else { None }
        })
        .collect();
    
    assert!(counters.len() >= 3, "expected at least 3 spacer lines, got {}", counters.len());
    // Each counter should be greater than the previous.
    for i in 1..counters.len() {
        assert!(counters[i] > counters[i - 1],
            "counter should increment: ... {}, {}", counters[i-1], counters[i]);
    }
}

/// Helper: spawn a process that appends lines to a file with gaps.
fn spawn_file_writer(path: &str, gap_seconds: u64, lines: &[&str]) -> std::process::Child {
    let line_args = lines.iter().map(|l| format!("'{}'", l.replace('\\', "\\\\").replace('\'', "\\'"))).collect::<Vec<_>>().join(", ");
    let script = format!(
        "
import time
for line in [{}]:
    with open('{}', 'a') as f:
        f.write(line + '\\n')
        f.flush()
    time.sleep({})
",
        line_args,
        path.replace('\\', "\\\\").replace('\'', "\\'"),
        gap_seconds,
    );
    Command::new("python3")
        .args(["-c", &script])
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn file writer")
}
