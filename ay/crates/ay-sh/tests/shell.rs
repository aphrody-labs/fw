//! The `ay-sh` binary as a shell: scripts, `-c`, exit statuses, and an interactive session on a real PTY.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const AYSH: &str = env!("CARGO_BIN_EXE_ay-sh");

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(AYSH)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("aysh did not start");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn it_is_bash_5_3() {
    let (code, out, err) = run(&["-c", "echo ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"]);
    assert_eq!((code, out.trim()), (0, "5.3"), "stderr: {err}");
}

#[test]
fn bash_expansions_work() {
    let script = r#"a=(one two three); declare -A m=([k]=v); f() { local x=$1; echo "${x^^}:${#a[@]}:${m[k]}:$((6*7))"; }; f hi"#;
    let (code, out, err) = run(&["-c", script]);
    assert_eq!((code, out.trim()), (0, "HI:3:v:42"), "stderr: {err}");
}

#[test]
fn the_exit_status_is_the_scripts() {
    assert_eq!(run(&["-c", "exit 7"]).0, 7);
    assert_eq!(run(&["-c", "false"]).0, 1);
    assert_eq!(run(&["-c", "(exit 3); echo $?"]).1.trim(), "3");
}

#[test]
fn a_script_file_runs_with_its_arguments() {
    let dir = std::env::temp_dir().join(format!("aysh-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("args.sh");
    std::fs::write(&file, "printf '%s|' \"$#\" \"$@\"\nexit 4\n").unwrap();
    let (code, out, err) = run(&[file.to_str().unwrap(), "a b", "c"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((code, out.as_str()), (4, "2|a b|c|"), "stderr: {err}");
}

#[test]
fn a_script_on_stdin_runs() {
    let mut child = Command::new(AYSH)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"for i in 1 2 3; do printf %s $i; done\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "123",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_interactive_session_on_a_pty_runs_commands_and_exits() {
    let session = ay_sh::pty::Session::spawn(
        &ay_sh::pty::Command::new(AYSH, ["--norc", "-i"]).env("PS1", "aysh> "),
    )
    .unwrap();
    session.write_line("echo interactive-$((20+22))");
    assert!(
        session.wait_for_text("interactive-42", Duration::from_secs(30)),
        "screen:\n{}",
        session.text()
    );
    session.write_line("exit 5");
    let exit = session.wait_for_exit(Duration::from_secs(30));
    assert_eq!(
        exit.and_then(|e| e.code),
        Some(5),
        "screen:\n{}",
        session.text()
    );
}
