//! Real programs on a real pseudo-terminal: `sh` on Unix, `cmd` on Windows.

use std::time::Duration;

use ay_sh::pty::{Command, Exit, Session};

const SOON: Duration = Duration::from_secs(20);

/// A one-shot script for the platform's stock shell.
fn script(unix: &str, windows: &str) -> Command {
    if cfg!(windows) {
        Command::new("cmd", ["/d", "/c", windows])
    } else {
        Command::new("sh", ["-c", unix])
    }
}

/// The platform's stock shell, interactive.
fn shell() -> Command {
    if cfg!(windows) {
        Command::new("cmd", ["/d", "/q"])
    } else {
        Command::new("sh", [] as [&str; 0]).env("PS1", "$ ")
    }
}

#[test]
fn output_reaches_the_screen_and_the_exit_code_is_reported() {
    let session = Session::spawn(&script(
        "echo hello from pty; exit 7",
        "echo hello from pty& exit 7",
    ))
    .unwrap();
    assert_eq!(
        session.wait_for_exit(SOON),
        Some(Exit { code: Some(7) }),
        "screen:\n{}",
        session.text()
    );
    assert!(
        session.wait_for_text("hello from pty", SOON),
        "screen:\n{}",
        session.text()
    );
}

#[test]
fn typed_input_runs_in_the_program() {
    let session = Session::spawn(&shell()).unwrap();
    // The echoed input never contains the expanded text, so a match is the program's own output.
    if cfg!(windows) {
        session.write_line("echo marker-%OS%");
        assert!(
            session.wait_for_text("marker-Windows_NT", SOON),
            "screen:\n{}",
            session.text()
        );
    } else {
        session.write_line("echo marker-$((40+2))");
        assert!(
            session.wait_for_text("marker-42", SOON),
            "screen:\n{}",
            session.text()
        );
    }
    session.write_line("exit");
    assert_eq!(
        session.wait_for_exit(SOON),
        Some(Exit { code: Some(0) }),
        "screen:\n{}",
        session.text()
    );
}

#[test]
fn a_resize_reaches_the_screen_and_the_program() {
    let session = Session::spawn(&shell().size(80, 24)).unwrap();
    assert_eq!(session.size(), (80, 24));
    assert_eq!(session.screen().len(), 24);

    session.resize(100, 30);
    assert_eq!(session.size(), (100, 30));
    assert_eq!(session.screen().len(), 30);

    // The program sees the new size too. `stty size` prints "rows cols"; cmd has no locale-neutral
    // equivalent, so Windows is checked on the terminal side only.
    if cfg!(unix) {
        session.write_line("stty size");
        assert!(
            session.wait_for_text("30 100", SOON),
            "screen:\n{}",
            session.text()
        );
    }
}

#[test]
fn a_wide_line_wraps_at_the_terminal_width() {
    let line = "x".repeat(30);
    let session =
        Session::spawn(&script(&format!("echo {line}"), &format!("echo {line}")).size(20, 5))
            .unwrap();
    assert!(
        session.wait_for_text(&"x".repeat(20), SOON),
        "screen:\n{}",
        session.text()
    );
    assert!(
        session
            .wait(SOON, |s| s
                .screen()
                .iter()
                .any(|row| row == &"x".repeat(10))
                .then_some(()))
            .is_some(),
        "screen:\n{}",
        session.text()
    );
}

#[test]
fn a_finished_command_and_the_title_are_reported() {
    // OSC 2 sets the title; OSC 133 A opens a prompt, C starts a command's output, D;3 ends it with status 3.
    let unix =
        r"printf '\033]2;aysh title\007\033]133;A\007prompt \033]133;C\007out\n\033]133;D;3\007'";
    let windows =
        "echo \x1b]2;aysh title\x07\x1b]133;A\x07prompt \x1b]133;C\x07out& echo \x1b]133;D;3\x07";
    let session = Session::spawn(&script(unix, windows)).unwrap();

    assert_eq!(
        session.wait(SOON, |s| s.commands().first().copied()),
        Some(Some(3)),
        "screen:\n{}",
        session.text()
    );
    assert!(
        session
            .wait(SOON, |s| (s.title().as_deref() == Some("aysh title"))
                .then_some(()))
            .is_some(),
        "title: {:?}",
        session.title()
    );
    assert!(
        session.wait_for_text("prompt out", SOON),
        "screen:\n{}",
        session.text()
    );
}

#[test]
fn shutdown_ends_a_program_that_would_run_forever() {
    let session = Session::spawn(&shell()).unwrap();
    if cfg!(windows) {
        session.write_line("echo ready-%OS%");
        assert!(
            session.wait_for_text("ready-Windows_NT", SOON),
            "screen:\n{}",
            session.text()
        );
    } else {
        session.write_line("echo ready-$((1+1))");
        assert!(
            session.wait_for_text("ready-2", SOON),
            "screen:\n{}",
            session.text()
        );
    }
    session.shutdown();
    assert!(
        session.wait_for_exit(SOON).is_some(),
        "the program survived shutdown"
    );
}

#[test]
fn a_terminal_without_cells_is_refused() {
    let error = Session::spawn(&shell().size(0, 24))
        .err()
        .expect("a zero-column terminal was accepted");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn a_missing_program_is_an_error_or_an_immediate_exit() {
    // Unix reports the failed exec through the child; ConPTY fails at creation.
    match Session::spawn(&Command::new("aysh-pty-no-such-program", [] as [&str; 0])) {
        Err(_) => {}
        Ok(session) => assert!(
            session.wait_for_exit(SOON).is_some(),
            "screen:\n{}",
            session.text()
        ),
    }
}
