//! A program running in a pseudo-terminal, together with the screen it draws.
//!
//! [`Session::spawn`] starts a program on a PTY (ConPTY on Windows, a Unix PTY elsewhere) and feeds
//! everything it prints to a headless terminal. The host then reads the screen as text, types into the
//! program, resizes it, and observes what the program reported through escape sequences: its title, the
//! commands a shell marked as finished (OSC 133) and its exit.
//!
//! The PTY and the terminal core are Rio's (`teletypewriter`, `rio-vt`); this crate only wires them and
//! gives the result a small, blocking API for hosts that are not terminal emulators: an agent surface,
//! a remote terminal server, tests.

use std::borrow::Cow;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::pos::Column;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::sync::FairMutex;
use rio_vt::event::{EventListener, Msg, RioEvent, WindowId, WindowSize};
use rio_vt::performer::Machine;

/// How often a wait re-reads the session.
const POLL: Duration = Duration::from_millis(10);

/// What to run and in which terminal.
#[derive(Debug, Clone)]
pub struct Command {
    /// Program to run. `None` runs the platform's default shell.
    pub program: Option<String>,
    pub args: Vec<String>,
    /// Working directory. `None` inherits the host's.
    pub cwd: Option<String>,
    /// Variables set on top of the inherited environment.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// Lines kept above the screen.
    pub scrollback: usize,
}

impl Command {
    /// `program` with `args` in an 80x24 terminal.
    pub fn new<P, I, A>(program: P, args: I) -> Command
    where
        P: Into<String>,
        I: IntoIterator<Item = A>,
        A: Into<String>,
    {
        Command {
            program: Some(program.into()),
            args: args.into_iter().map(Into::into).collect(),
            ..Command::default()
        }
    }

    pub fn size(mut self, cols: u16, rows: u16) -> Command {
        self.cols = cols;
        self.rows = rows;
        self
    }

    pub fn cwd(mut self, cwd: impl Into<String>) -> Command {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, name: impl Into<String>, value: impl Into<String>) -> Command {
        self.env.push((name.into(), value.into()));
        self
    }
}

impl Default for Command {
    fn default() -> Command {
        Command {
            program: None,
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            cols: 80,
            rows: 24,
            scrollback: 10_000,
        }
    }
}

/// How the program ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exit {
    /// The exit code, when the platform reported one and the program was not killed by a signal.
    pub code: Option<i32>,
}

/// What the program reported through escape sequences.
#[derive(Debug, Default)]
struct Reported {
    title: Option<String>,
    /// OSC 133 `D` marks seen, each with the exit status the shell attached to it.
    commands: Vec<Option<i32>>,
    exit: Option<Exit>,
    closed: bool,
}

type Writer = Arc<Mutex<Option<Box<dyn Fn(Msg) + Send>>>>;

#[derive(Clone)]
struct Listener {
    reported: Arc<Mutex<Reported>>,
    /// Set once the reader thread exists: terminal replies (cursor position, device attributes) go back
    /// to the program through it. ConPTY asks for the cursor position before it prints anything.
    writer: Writer,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Listener {
    fn on(&self, event: RioEvent) {
        match event {
            RioEvent::PtyWrite(_, text) => {
                if let Some(write) = lock(&self.writer).as_ref() {
                    write(Msg::Input(Cow::Owned(text.into_bytes())));
                }
            }
            RioEvent::Title(_, title) => lock(&self.reported).title = Some(title),
            RioEvent::CommandFinished { exit_status, .. } => {
                lock(&self.reported).commands.push(exit_status)
            }
            RioEvent::ChildExited(_, status) => {
                lock(&self.reported).exit = Some(Exit {
                    code: exit_code(status),
                })
            }
            RioEvent::CloseTerminal(_) | RioEvent::Exit => lock(&self.reported).closed = true,
            _ => {}
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: RioEvent, _: WindowId) {
        self.on(event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, _: WindowId) {
        self.on(event);
    }

    fn send_global_event(&self, event: RioEvent) {
        self.on(event);
    }
}

/// The exit code behind the status the PTY reports: a raw wait status on Unix, the exit code on Windows.
#[cfg(unix)]
fn exit_code(status: Option<i32>) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.and_then(|raw| std::process::ExitStatus::from_raw(raw).code())
}

#[cfg(not(unix))]
fn exit_code(status: Option<i32>) -> Option<i32> {
    status
}

/// A running program and its terminal.
pub struct Session {
    terminal: Arc<FairMutex<Crosswords<Listener>>>,
    reported: Arc<Mutex<Reported>>,
    writer: Writer,
    size: Mutex<(u16, u16)>,
    /// The reader thread. It owns the PTY and ends with the program, or on shutdown.
    reader: Mutex<Option<JoinHandle<()>>>,
}

impl Session {
    /// Starts `command` on a new PTY.
    pub fn spawn(command: &Command) -> io::Result<Session> {
        if command.cols == 0 || command.rows == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a terminal has at least one column and one row",
            ));
        }
        let reported = Arc::new(Mutex::new(Reported::default()));
        let writer: Writer = Arc::new(Mutex::new(None));
        let listener = Listener {
            reported: Arc::clone(&reported),
            writer: Arc::clone(&writer),
        };

        let terminal = Crosswords::new(
            CrosswordsSize::new(usize::from(command.cols), usize::from(command.rows)),
            CursorShape::Block,
            listener.clone(),
            WindowId::from(0),
            0,
            command.scrollback,
        );
        let terminal = Arc::new(FairMutex::new(terminal));

        let pty = open(command)?;
        let machine = Machine::new(Arc::clone(&terminal), pty, listener, WindowId::from(0), 0)
            .map_err(|error| io::Error::other(error.to_string()))?;
        let channel = machine.channel();
        *lock(&writer) = Some(Box::new(move |message| {
            let _ = channel.send(message);
        }));
        let io = machine.spawn();
        let reader = thread::Builder::new()
            .name("aysh-pty reader".to_string())
            .spawn(move || {
                // Joining here drops the machine, and with it the PTY, as soon as the loop ends.
                let _ = io.join();
            })?;

        Ok(Session {
            terminal,
            reported,
            writer,
            size: Mutex::new((command.cols, command.rows)),
            reader: Mutex::new(Some(reader)),
        })
    }

    fn send(&self, message: Msg) {
        if let Some(write) = lock(&self.writer).as_ref() {
            write(message);
        }
    }

    /// Types `bytes` into the program.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.send(Msg::Input(bytes.into()));
    }

    /// Types `line` followed by Enter.
    pub fn write_line(&self, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\r');
        self.write(bytes);
    }

    /// Resizes the terminal and tells the program.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        self.terminal
            .lock()
            .resize(CrosswordsSize::new(usize::from(cols), usize::from(rows)));
        self.send(Msg::Resize(WindowSize {
            rows,
            cols,
            width: 0,
            height: 0,
        }));
        *lock(&self.size) = (cols, rows);
    }

    /// Columns and rows.
    pub fn size(&self) -> (u16, u16) {
        *lock(&self.size)
    }

    /// The visible rows, top to bottom, without trailing blanks.
    pub fn screen(&self) -> Vec<String> {
        let terminal = self.terminal.lock();
        let cols = terminal.columns();
        terminal
            .visible_rows()
            .iter()
            .map(|row| {
                // A cell nothing was written to holds NUL.
                let text: String = (0..cols)
                    .map(|x| row[Column(x)].c())
                    .map(|c| if c == '\0' { ' ' } else { c })
                    .collect();
                text.trim_end().to_string()
            })
            .collect()
    }

    /// The screen as one string, rows joined by `\n`, without trailing blank rows.
    pub fn text(&self) -> String {
        let mut rows = self.screen();
        while rows.last().is_some_and(String::is_empty) {
            rows.pop();
        }
        rows.join("\n")
    }

    /// The title the program set (OSC 0 or 2), if any.
    pub fn title(&self) -> Option<String> {
        lock(&self.reported).title.clone()
    }

    /// Commands the shell reported as finished, oldest first, each with the exit status it attached: an OSC 133 `D`
    /// that follows a `C`. The core reports nothing for a prompt mark (`A`) alone.
    pub fn commands(&self) -> Vec<Option<i32>> {
        lock(&self.reported).commands.clone()
    }

    /// `Some` once the program has ended.
    pub fn exit(&self) -> Option<Exit> {
        let reported = lock(&self.reported);
        reported.exit.or(if reported.closed {
            Some(Exit { code: None })
        } else {
            None
        })
    }

    /// Blocks until `ready` returns `Some`, or `timeout` passes.
    pub fn wait<T>(
        &self,
        timeout: Duration,
        mut ready: impl FnMut(&Session) -> Option<T>,
    ) -> Option<T> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(value) = ready(self) {
                return Some(value);
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(POLL);
        }
    }

    /// Blocks until `needle` is on the screen. Returns whether it appeared in time.
    pub fn wait_for_text(&self, needle: &str, timeout: Duration) -> bool {
        self.wait(timeout, |session| {
            session
                .screen()
                .iter()
                .any(|row| row.contains(needle))
                .then_some(())
        })
        .is_some()
    }

    /// Blocks until the program ends.
    pub fn wait_for_exit(&self, timeout: Duration) -> Option<Exit> {
        self.wait(timeout, Session::exit)
    }

    /// Ends the program and waits for it: hangup, then kill on Unix; closing the pseudo-console on Windows.
    /// The reader reports nothing when it is told to stop, so the end is recorded here.
    pub fn shutdown(&self) {
        self.send(Msg::Shutdown);
        if let Some(reader) = lock(&self.reader).take() {
            let _ = reader.join();
        }
        lock(&self.reported).closed = true;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn environment(command: &Command) -> Option<Vec<(String, String)>> {
    let mut env = command.env.clone();
    // A host that is not itself a terminal has no TERM to pass on.
    if cfg!(unix) && !env.iter().any(|(name, _)| name == "TERM") {
        env.push(("TERM".to_string(), "xterm-256color".to_string()));
        env.push(("COLORTERM".to_string(), "truecolor".to_string()));
    }
    if env.is_empty() { None } else { Some(env) }
}

#[cfg(unix)]
fn open(command: &Command) -> io::Result<rio_vt::teletypewriter::Pty> {
    rio_vt::teletypewriter::create_pty_with_spawn(
        command.program.as_deref(),
        command.args.clone(),
        &command.cwd,
        environment(command),
        command.cols,
        command.rows,
        0,
        0,
    )
}

#[cfg(windows)]
fn open(command: &Command) -> io::Result<rio_vt::teletypewriter::Pty> {
    // teletypewriter joins the program and its arguments with spaces into one command line, so each one
    // is quoted here by the rules CommandLineToArgvW reverses.
    let program = command.program.as_deref().map(quote);
    let args = command.args.iter().map(|arg| quote(arg)).collect();
    rio_vt::teletypewriter::create_pty(
        program.as_deref(),
        args,
        &command.cwd,
        environment(command),
        command.cols,
        command.rows,
    )
}

/// Quotes one argument for a Windows command line.
#[cfg_attr(not(windows), allow(dead_code))]
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                // The backslashes already written are doubled, and one more escapes the quote.
                out.extend(std::iter::repeat_n('\\', backslashes + 1));
                out.push('"');
                backslashes = 0;
                continue;
            }
            _ => backslashes = 0,
        }
        out.push(c);
    }
    // Backslashes before the closing quote would escape it.
    out.extend(std::iter::repeat_n('\\', backslashes));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn quoting_follows_the_windows_command_line_rules() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("two words"), "\"two words\"");
        assert_eq!(quote(r"C:\dir\"), r"C:\dir\");
        assert_eq!(quote(r"C:\my dir\"), r#""C:\my dir\\""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(r#"a\"b"#), r#""a\\\"b""#);
    }
}
