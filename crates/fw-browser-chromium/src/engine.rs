//! The CEF-bound half: process bootstrap, the engine thread, and one browser per page. Every CEF callback runs on
//! CEF's UI thread, where the page state lives in `Rc`s; only `Send` data crosses to the caller.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CString, c_char};
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use cef::*;

use crate::{CefDir, Config, Error, Result};

/// A painted frame: BGRA pixels, top row first.
pub(crate) struct Pixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

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
            .name("browser-chromium-cef".into())
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
}

impl Engine {
    /// Close every browser and shut CEF down; later jobs fail with `Error::Closed`.
    pub(crate) fn close(&self) {
        // Pages close from tasks on the UI thread; CEF must not shut down while one is still open.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while LIVE_PAGES.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
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
        None => std::env::temp_dir().join(format!("browser-chromium-{}", std::process::id())),
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

// ---- one browser per page (UI thread) ----

static NEXT_PAGE: AtomicU64 = AtomicU64::new(1);
static LIVE_PAGES: AtomicUsize = AtomicUsize::new(0);

const DEFAULT_VIEW: (i32, i32) = (1280, 720);

struct PageState {
    browser: Option<Browser>,
    observer: Option<Registration>,
    size: (i32, i32),
    open: Option<Sender<Result<()>>>,
    loaded: bool,
    next_message: i32,
    evals: HashMap<i32, Sender<Result<serde_json::Value>>>,
    shot: Option<PendingShot>,
}

/// A screenshot waiting for the frame of the requested size.
type PendingShot = (Sender<Result<Pixels>>, (i32, i32));

type Shared = Rc<RefCell<PageState>>;

thread_local! {
    /// Pages by id, owned by the UI thread.
    static PAGES: RefCell<HashMap<u64, Shared>> = RefCell::new(HashMap::new());
}

fn page(id: u64) -> Option<Shared> {
    PAGES.with(|pages| pages.borrow().get(&id).cloned())
}

/// Answer the pending `open`, once.
fn answer_open(state: &Shared, result: Result<()>) {
    let reply = state.borrow_mut().open.take();
    if let Some(reply) = reply {
        let _ = reply.send(result);
    }
}

fn host_of(state: &Shared) -> Option<BrowserHost> {
    state.borrow().browser.as_ref().and_then(|b| b.host())
}

/// A handle to one open page; closing the page is dropping the handle.
pub(crate) struct PageHandle {
    id: u64,
}

fn post(task: &mut Task) -> Result<()> {
    if post_task(ThreadId::UI, Some(task)) == 1 {
        Ok(())
    } else {
        Err(Error::Closed)
    }
}

impl PageHandle {
    /// Create a browser on `url` and wait for its main frame to load.
    pub(crate) fn open(url: &str, timeout: Duration) -> Result<Self> {
        let id = NEXT_PAGE.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        LIVE_PAGES.fetch_add(1, Ordering::SeqCst);
        let handle = Self { id };
        post(&mut OpenPage::new(id, url.to_owned(), timeout, tx))?;
        rx.recv_timeout(timeout + Duration::from_secs(10))
            .map_err(|_| Error::Timeout(timeout))??;
        Ok(handle)
    }

    pub(crate) fn evaluate(
        &self,
        expression: &str,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let (tx, rx) = mpsc::channel();
        post(&mut EvaluatePage::new(self.id, expression.to_owned(), tx))?;
        rx.recv_timeout(timeout)
            .map_err(|_| Error::Timeout(timeout))?
    }

    pub(crate) fn screenshot(&self, width: u32, height: u32, timeout: Duration) -> Result<Pixels> {
        let (tx, rx) = mpsc::channel();
        post(&mut PaintPage::new(
            self.id,
            width as i32,
            height as i32,
            tx,
        ))?;
        rx.recv_timeout(timeout)
            .map_err(|_| Error::Timeout(timeout))?
    }
}

impl Drop for PageHandle {
    fn drop(&mut self) {
        let mut task = ClosePage::new(self.id);
        if post(&mut task).is_err() {
            LIVE_PAGES.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

wrap_task! {
    struct OpenPage {
        id: u64,
        url: String,
        timeout: Duration,
        reply: Sender<Result<()>>,
    }

    impl Task {
        fn execute(&self) {
            let state: Shared = Rc::new(RefCell::new(PageState {
                browser: None,
                observer: None,
                size: DEFAULT_VIEW,
                open: Some(self.reply.clone()),
                loaded: false,
                next_message: 1,
                evals: HashMap::new(),
                shot: None,
            }));
            PAGES.with(|pages| pages.borrow_mut().insert(self.id, state.clone()));
            let mut client = PageClient::new(
                PageLifeSpan::new(state.clone()),
                PageLoad::new(state.clone()),
                PageRender::new(state.clone()),
            );
            let window_info = WindowInfo::default().set_as_windowless(Default::default());
            let settings = BrowserSettings { windowless_frame_rate: 30, ..Default::default() };
            let created = browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&CefString::from(self.url.as_str())),
                Some(&settings),
                None,
                None,
            );
            if created != 1 {
                answer_open(&state, Err(Error::Start("cef refused to create a browser".into())));
                return;
            }
            let mut watchdog = OpenWatchdog::new(state, self.timeout);
            post_delayed_task(ThreadId::UI, Some(&mut watchdog), self.timeout.as_millis() as i64);
        }
    }
}

wrap_task! {
    struct OpenWatchdog {
        state: Shared,
        timeout: Duration,
    }

    impl Task {
        fn execute(&self) {
            answer_open(&self.state, Err(Error::Timeout(self.timeout)));
        }
    }
}

wrap_task! {
    struct EvaluatePage {
        id: u64,
        expression: String,
        reply: Sender<Result<serde_json::Value>>,
    }

    impl Task {
        fn execute(&self) {
            let fail = |e: Error| {
                let _ = self.reply.send(Err(e));
            };
            let Some(state) = page(self.id) else { return fail(Error::Closed) };
            let Some(host) = host_of(&state) else { return fail(Error::Closed) };
            let Some(mut params) = dictionary_value_create() else {
                return fail(Error::Protocol("cannot allocate DevTools params".into()));
            };
            params.set_string(Some(&"expression".into()), Some(&CefString::from(self.expression.as_str())));
            params.set_bool(Some(&"returnByValue".into()), 1);
            params.set_bool(Some(&"awaitPromise".into()), 1);
            let message = {
                let mut st = state.borrow_mut();
                let message = st.next_message;
                st.next_message += 1;
                st.evals.insert(message, self.reply.clone());
                message
            };
            if host.execute_dev_tools_method(message, Some(&"Runtime.evaluate".into()), Some(&mut params)) == 0 {
                state.borrow_mut().evals.remove(&message);
                fail(Error::Protocol("Runtime.evaluate was rejected".into()));
            }
        }
    }
}

wrap_task! {
    struct PaintPage {
        id: u64,
        width: i32,
        height: i32,
        reply: Sender<Result<Pixels>>,
    }

    impl Task {
        fn execute(&self) {
            let Some(state) = page(self.id) else {
                let _ = self.reply.send(Err(Error::Closed));
                return;
            };
            let Some(host) = host_of(&state) else {
                let _ = self.reply.send(Err(Error::Closed));
                return;
            };
            let resized = {
                let mut st = state.borrow_mut();
                st.shot = Some((self.reply.clone(), (self.width, self.height)));
                std::mem::replace(&mut st.size, (self.width, self.height)) != (self.width, self.height)
            };
            if resized {
                host.was_resized();
            }
            host.invalidate(PaintElementType::VIEW);
        }
    }
}

wrap_task! {
    struct ClosePage {
        id: u64,
    }

    impl Task {
        fn execute(&self) {
            let state = PAGES.with(|pages| pages.borrow_mut().remove(&self.id));
            match state.and_then(|s| host_of(&s)) {
                Some(host) => host.close_browser(1),
                None => {
                    LIVE_PAGES.fetch_sub(1, Ordering::SeqCst);
                }
            }
        }
    }
}

wrap_client! {
    struct PageClient {
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
    struct PageLifeSpan {
        state: Shared,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let mut observer = PageDevTools::new(self.state.clone());
            let registration = browser.host().and_then(|h| h.add_dev_tools_message_observer(Some(&mut observer)));
            let mut st = self.state.borrow_mut();
            st.browser = Some(browser.clone());
            st.observer = registration;
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            // Break the browser <-> state reference cycle and release the pending callers.
            let (evals, shot) = {
                let mut st = self.state.borrow_mut();
                st.browser = None;
                st.observer = None;
                (std::mem::take(&mut st.evals), st.shot.take())
            };
            for (_, reply) in evals {
                let _ = reply.send(Err(Error::Closed));
            }
            if let Some((reply, _)) = shot {
                let _ = reply.send(Err(Error::Closed));
            }
            LIVE_PAGES.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

wrap_load_handler! {
    struct PageLoad {
        state: Shared,
    }

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, _status: i32) {
            if frame.is_none_or(|f| f.is_main() != 1) {
                return;
            }
            self.state.borrow_mut().loaded = true;
            answer_open(&self.state, Ok(()));
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
            answer_open(&self.state, Err(Error::Load(format!("{text} ({url})"))));
        }
    }
}

wrap_render_handler! {
    struct PageRender {
        state: Shared,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let (width, height) = self.state.borrow().size;
                *rect = Rect { x: 0, y: 0, width, height };
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
            let reply = {
                let mut st = self.state.borrow_mut();
                // Only a frame at the requested size answers; the one before a resize does not.
                match st.shot.take_if(|(_, size)| *size == (width, height)) {
                    Some((reply, _)) => reply,
                    None => return,
                }
            };
            // SAFETY: CEF guarantees `buffer` holds width * height BGRA pixels for the duration of the callback.
            let bgra = unsafe { std::slice::from_raw_parts(buffer, width as usize * height as usize * 4) }.to_vec();
            let _ = reply.send(Ok(Pixels { width: width as u32, height: height as u32, bgra }));
        }
    }
}

wrap_dev_tools_message_observer! {
    struct PageDevTools {
        state: Shared,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(&self, _browser: Option<&mut Browser>, message_id: i32, success: i32, result: Option<&[u8]>) {
            let reply = self.state.borrow_mut().evals.remove(&message_id);
            if let Some(reply) = reply {
                let _ = reply.send(parse_evaluate(success == 1, result.unwrap_or_default()));
            }
        }
    }
}

/// `Runtime.evaluate` with `returnByValue` replies `{result: {type, value}}` or `{exceptionDetails: ...}`.
fn parse_evaluate(success: bool, raw: &[u8]) -> Result<serde_json::Value> {
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
    Ok(reply
        .pointer("/result/value")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(raw: &str) -> Result<serde_json::Value> {
        parse_evaluate(true, raw.as_bytes())
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
