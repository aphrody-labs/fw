//! The CEF-bound half: process bootstrap, the engine thread, and one browser per job. Every CEF callback runs on
//! CEF's UI thread, where the per-job state lives in `Rc`s; only `Send` data crosses to the caller.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::{CString, c_char};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use cef::*;

use crate::{CefDir, Config, Error, Result};

#[derive(Clone)]
pub(crate) enum Job {
    Screenshot {
        url: String,
        width: u32,
        height: u32,
        settle: Duration,
        timeout: Duration,
    },
    Evaluate {
        url: String,
        expression: String,
        settle: Duration,
        timeout: Duration,
    },
}

impl Job {
    fn timeout(&self) -> Duration {
        match self {
            Job::Screenshot { timeout, .. } | Job::Evaluate { timeout, .. } => *timeout,
        }
    }
}

pub(crate) enum Output {
    Frame {
        width: u32,
        height: u32,
        bgra: Vec<u8>,
    },
    Value(serde_json::Value),
}

type Reply = Sender<Result<Output>>;

/// Run CEF's child-process entry. Call first thing in `main` of an executable that CEF re-launches: returns
/// `Some(exit_code)` in a child process (exit with it) and `None` in the browser process.
pub fn subprocess_main() -> Option<i32> {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let code = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());
    (code >= 0).then_some(code)
}

pub(crate) struct Engine {
    shutdown: Mutex<Option<Sender<()>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub(crate) fn start(config: Config) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("ay-chromium-cef".into())
            .spawn(move || {
                // CEF is initialized and shut down on this thread; its own UI thread runs the message loop.
                match boot(&config) {
                    Ok(guard) => {
                        let _ = ready_tx.send(Ok(()));
                        let _ = shutdown_rx.recv();
                        drop(guard);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })?;
        ready_rx.recv().map_err(|_| Error::Closed)??;
        Ok(Self {
            shutdown: Mutex::new(Some(shutdown_tx)),
            thread: Mutex::new(Some(thread)),
        })
    }

    pub(crate) fn run(&self, job: Job) -> Result<Output> {
        let (tx, rx) = mpsc::channel();
        let timeout = job.timeout();
        let mut task = StartJob::new(job, tx);
        if post_task(ThreadId::UI, Some(&mut task)) != 1 {
            return Err(Error::Closed);
        }
        // The watchdog on the UI thread reports the timeout; the grace period only covers a stuck UI thread.
        rx.recv_timeout(timeout + Duration::from_secs(10))
            .map_err(|_| Error::Timeout(timeout))?
    }
}

impl Engine {
    /// Close every browser and shut CEF down; later jobs fail with `Error::Closed`.
    pub(crate) fn close(&self) {
        if let Some(tx) = self.shutdown.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(());
        }
        if let Some(thread) = self.thread.lock().ok().and_then(|mut g| g.take()) {
            let _ = thread.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.close();
    }
}

/// Keeps the argv CEF reads alive and shuts CEF down on drop.
struct CefGuard {
    _argv: Vec<CString>,
    _ptrs: Vec<*const c_char>,
}

impl Drop for CefGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

wrap_app! {
    struct EngineApp {
        ready: Sender<()>,
    }

    impl App {
        fn on_before_command_line_processing(&self, _process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            let Some(line) = command_line else { return };
            for switch in [
                "ozone-platform=headless",
                "disable-gpu",
                "disable-gpu-compositing",
                "disable-dev-shm-usage",
                "disable-extensions",
                "disable-background-networking",
                "mute-audio",
                "log-level=3",
                "noerrdialogs",
                "use-mock-keychain",
            ] {
                match switch.split_once('=') {
                    Some((name, value)) => line.append_switch_with_value(Some(&name.into()), Some(&value.into())),
                    None => line.append_switch(Some(&switch.into())),
                }
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ReadyHandler::new(self.ready.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct ReadyHandler {
        ready: Sender<()>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let _ = self.ready.send(());
        }
    }
}

fn boot(config: &Config) -> Result<CefGuard> {
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let helper = match &config.helper {
        Some(path) => path.clone(),
        None => std::env::current_exe()?,
    };
    let cef_dir = CefDir::locate(config.cef_dir.as_deref(), std::slice::from_ref(&helper))?;
    let cache = match &config.cache_dir {
        Some(dir) => dir.clone(),
        None => std::env::temp_dir().join(format!("ay-chromium-{}", std::process::id())),
    };
    std::fs::create_dir_all(&cache)?;

    // CEF reads an argv in the browser process; hand it a controlled one instead of the host's (Bun's).
    let argv = vec![
        CString::new(helper.to_string_lossy().as_bytes())
            .map_err(|e| Error::Start(e.to_string()))?,
    ];
    let ptrs: Vec<*const c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    let main_args = MainArgs {
        argc: ptrs.len() as i32,
        argv: ptrs.as_ptr() as *mut *mut c_char,
    };

    let settings = Settings {
        no_sandbox: 1,
        multi_threaded_message_loop: 1,
        windowless_rendering_enabled: 1,
        command_line_args_disabled: 1,
        disable_signal_handlers: 1,
        browser_subprocess_path: CefString::from(helper.to_string_lossy().as_ref()),
        resources_dir_path: CefString::from(cef_dir.path().to_string_lossy().as_ref()),
        locales_dir_path: CefString::from(
            cef_dir.path().join("locales").to_string_lossy().as_ref(),
        ),
        root_cache_path: CefString::from(cache.to_string_lossy().as_ref()),
        cache_path: CefString::from(cache.join("profile").to_string_lossy().as_ref()),
        log_file: CefString::from(cache.join("cef.log").to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        ..Default::default()
    };

    let (ready_tx, ready_rx) = mpsc::channel();
    let mut app = EngineApp::new(ready_tx);
    if initialize(
        Some(&main_args),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err(Error::Start(format!(
            "cef initialize failed (log: {})",
            cache.join("cef.log").display()
        )));
    }
    let guard = CefGuard {
        _argv: argv,
        _ptrs: ptrs,
    };
    ready_rx
        .recv_timeout(Duration::from_secs(30))
        .map_err(|_| Error::Start("CEF context did not initialize in 30 s".into()))?;
    Ok(guard)
}

// ---- one browser per job (UI thread) ----

struct JobState {
    job: Job,
    reply: Reply,
    browser: Option<Browser>,
    observer: Option<Registration>,
    loaded: bool,
    armed: bool,
    finished: bool,
}

type Shared = Rc<RefCell<JobState>>;

fn finish(state: &Shared, result: Result<Output>) {
    let browser = {
        let mut st = state.borrow_mut();
        if st.finished {
            return;
        }
        st.finished = true;
        let _ = st.reply.send(result);
        st.browser.clone()
    };
    // Close from a fresh task: closing inside the callback that finished the job re-enters CEF's observer lists
    // and the life-span handler.
    if let Some(browser) = browser {
        let mut task = CloseBrowser::new(browser);
        post_task(ThreadId::UI, Some(&mut task));
    }
}

wrap_task! {
    struct CloseBrowser {
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            if let Some(host) = self.browser.host() {
                host.close_browser(1);
            }
        }
    }
}

wrap_task! {
    struct StartJob {
        job: Job,
        reply: Reply,
    }

    impl Task {
        fn execute(&self) {
            let state: Shared = Rc::new(RefCell::new(JobState {
                job: self.job.clone(),
                reply: self.reply.clone(),
                browser: None,
                observer: None,
                loaded: false,
                armed: false,
                finished: false,
            }));
            let (url, width, height) = match &self.job {
                Job::Screenshot { url, width, height, .. } => (url.clone(), *width, *height),
                Job::Evaluate { url, .. } => (url.clone(), 1280, 720),
            };
            let mut client = JobClient::new(
                JobLifeSpan::new(state.clone()),
                JobLoad::new(state.clone()),
                JobRender::new(state.clone(), width as i32, height as i32),
            );
            let window_info = WindowInfo::default().set_as_windowless(Default::default());
            let settings = BrowserSettings { windowless_frame_rate: 30, ..Default::default() };
            let created = browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&CefString::from(url.as_str())),
                Some(&settings),
                None,
                None,
            );
            if created != 1 {
                finish(&state, Err(Error::Start("cef refused to create a browser".into())));
                return;
            }
            let mut watchdog = Watchdog::new(state);
            post_delayed_task(ThreadId::UI, Some(&mut watchdog), self.job.timeout().as_millis() as i64);
        }
    }
}

wrap_task! {
    struct Watchdog {
        state: Shared,
    }

    impl Task {
        fn execute(&self) {
            let timeout = self.state.borrow().job.timeout();
            finish(&self.state, Err(Error::Timeout(timeout)));
        }
    }
}

wrap_task! {
    struct AfterLoad {
        state: Shared,
    }

    impl Task {
        fn execute(&self) {
            let (browser, job) = {
                let st = self.state.borrow();
                (st.browser.clone(), st.job.clone())
            };
            let Some(host) = browser.and_then(|b| b.host()) else { return };
            match job {
                Job::Screenshot { .. } => {
                    self.state.borrow_mut().armed = true;
                    host.invalidate(PaintElementType::VIEW);
                }
                Job::Evaluate { expression, .. } => {
                    let Some(params) = dictionary_value_create() else {
                        return finish(&self.state, Err(Error::Protocol("cannot allocate DevTools params".into())));
                    };
                    params.set_string(Some(&"expression".into()), Some(&CefString::from(expression.as_str())));
                    params.set_bool(Some(&"returnByValue".into()), 1);
                    params.set_bool(Some(&"awaitPromise".into()), 1);
                    let mut params = params;
                    if host.execute_dev_tools_method(EVAL_ID, Some(&"Runtime.evaluate".into()), Some(&mut params)) == 0 {
                        finish(&self.state, Err(Error::Protocol("Runtime.evaluate was rejected".into())));
                    }
                }
            }
        }
    }
}

const EVAL_ID: i32 = 1;

wrap_client! {
    struct JobClient {
        life_span: LifeSpanHandler,
        load: LoadHandler,
        render: RenderHandler,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }

        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }
    }
}

wrap_life_span_handler! {
    struct JobLifeSpan {
        state: Shared,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let mut st = self.state.borrow_mut();
            st.browser = Some(browser.clone());
            if matches!(st.job, Job::Evaluate { .. }) {
                let mut observer = JobDevTools::new(self.state.clone());
                st.observer = browser.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut observer)));
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            // Break the browser <-> state reference cycle.
            let mut st = self.state.borrow_mut();
            st.browser = None;
            st.observer = None;
        }
    }
}

wrap_load_handler! {
    struct JobLoad {
        state: Shared,
    }

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _status: i32) {
            if frame.is_none_or(|f| f.is_main() != 1) {
                return;
            }
            let settle = {
                let mut st = self.state.borrow_mut();
                if st.loaded {
                    return;
                }
                st.loaded = true;
                match st.job {
                    Job::Screenshot { settle, .. } | Job::Evaluate { settle, .. } => settle,
                }
            };
            let mut task = AfterLoad::new(self.state.clone());
            post_delayed_task(ThreadId::UI, Some(&mut task), settle.as_millis() as i64);
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            // ERR_ABORTED (-3) is a navigation superseded by another one, not a failure.
            if frame.is_none_or(|f| f.is_main() != 1) || error_code == Errorcode::ABORTED {
                return;
            }
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
            finish(&self.state, Err(Error::Load(format!("{text} ({url})"))));
        }
    }
}

wrap_render_handler! {
    struct JobRender {
        state: Shared,
        width: i32,
        height: i32,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                *rect = Rect { x: 0, y: 0, width: self.width, height: self.height };
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: i32,
            height: i32,
        ) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            if !self.state.borrow().armed {
                return;
            }
            // SAFETY: CEF guarantees `buffer` holds width * height BGRA pixels for the duration of the callback.
            let bgra = unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) }.to_vec();
            finish(&self.state, Ok(Output::Frame { width: width as u32, height: height as u32, bgra }));
        }
    }
}

wrap_dev_tools_message_observer! {
    struct JobDevTools {
        state: Shared,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: i32, success: i32, result: Option<&[u8]>) {
            if message_id != EVAL_ID {
                return;
            }
            finish(&self.state, parse_evaluate(success == 1, result.unwrap_or_default()));
        }
    }
}

/// `Runtime.evaluate` with `returnByValue` replies `{result: {type, value}}` or `{exceptionDetails: ...}`.
fn parse_evaluate(success: bool, raw: &[u8]) -> Result<Output> {
    let reply: serde_json::Value =
        serde_json::from_slice(raw).map_err(|e| Error::Protocol(e.to_string()))?;
    if !success {
        return Err(Error::Protocol(reply.to_string()));
    }
    if let Some(details) = reply.get("exceptionDetails") {
        let text = details
            .pointer("/exception/description")
            .or_else(|| details.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("exception");
        return Err(Error::Script(text.to_owned()));
    }
    Ok(Output::Value(
        reply
            .pointer("/result/value")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(raw: &str) -> Result<serde_json::Value> {
        match parse_evaluate(true, raw.as_bytes())? {
            Output::Value(v) => Ok(v),
            Output::Frame { .. } => unreachable!(),
        }
    }

    #[test]
    fn evaluate_reply_becomes_the_value() {
        assert_eq!(
            value(r#"{"result":{"type":"string","value":"Example Domain"}}"#).unwrap(),
            serde_json::json!("Example Domain")
        );
        assert_eq!(
            value(r#"{"result":{"type":"undefined"}}"#).unwrap(),
            serde_json::Value::Null
        );
    }

    #[test]
    fn evaluate_exception_is_a_script_error() {
        let raw = r#"{"result":{"type":"object"},"exceptionDetails":{"text":"Uncaught","exception":{"description":"ReferenceError: x is not defined"}}}"#;
        assert!(
            matches!(value(raw), Err(Error::Script(m)) if m == "ReferenceError: x is not defined")
        );
    }

    #[test]
    fn failed_method_is_a_protocol_error() {
        assert!(matches!(
            parse_evaluate(false, br#"{"code":-32000}"#),
            Err(Error::Protocol(_))
        ));
        assert!(matches!(
            parse_evaluate(true, b"not json"),
            Err(Error::Protocol(_))
        ));
    }
}
