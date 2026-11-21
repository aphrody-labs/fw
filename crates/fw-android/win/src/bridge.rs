//! Win32 client of fwa-stream v1: one top-level window per guest window, frames blitted from a DIB section, input sent back.
//! The UI thread owns every window and all state; a reader thread decodes the stream and wakes the UI thread through a
//! message-only window, so no Win32 handle ever crosses threads.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use fw_android_stream::{DecodeError, Message};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    EndPaint, GetDC, HBITMAP, HDC, HGDIOBJ, InvalidateRect, PAINTSTRUCT, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, ReleaseCapture, SetCapture, VIRTUAL_KEY};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const WINDOW_CLASS: PCWSTR = w!("FwAndroidWindow");
const SINK_CLASS: PCWSTR = w!("FwAndroidSink");
const WAKE: u32 = WM_APP + 1;
const BUTTON_LEFT: u8 = 1;
const BUTTON_RIGHT: u8 = 2;
const BUTTON_MIDDLE: u8 = 4;

enum Event {
    Message(Message),
    Closed(String),
}

struct Inbox {
    queue: Mutex<VecDeque<Event>>,
    sink: Mutex<isize>,
}

impl Inbox {
    fn push(&self, event: Event) {
        let was_empty = {
            let Ok(mut queue) = self.queue.lock() else { return };
            let was_empty = queue.is_empty();
            queue.push_back(event);
            was_empty
        };
        if was_empty {
            let sink = self.sink.lock().map(|guard| *guard).unwrap_or_default();
            // SAFETY: posting to a window handle from another thread is allowed; a stale handle only fails the call.
            unsafe { PostMessageW(Some(HWND(sink as *mut _)), WAKE, WPARAM(0), LPARAM(0)) }.ok();
        }
    }
}

struct Surface {
    hwnd: HWND,
    width: u32,
    height: u32,
    memory_dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
    bits: *mut u8,
    buttons: u8,
}

struct State {
    surfaces: HashMap<u32, Surface>,
    by_window: HashMap<isize, u32>,
    writer: Arc<Mutex<TcpStream>>,
    inbox: Arc<Inbox>,
    exit_code: i32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    // Set while the bridge itself resizes a window so WM_SIZE does not echo the change back as a ResizeRequest.
    static PROGRAMMATIC_RESIZE: Cell<bool> = const { Cell::new(false) };
    static SIZE_MOVE: Cell<bool> = const { Cell::new(false) };
}

fn with_state<R>(action: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| cell.borrow_mut().as_mut().map(action))
}

fn send(message: &Message) {
    let Some(writer) = with_state(|state| state.writer.clone()) else { return };
    let Ok(mut stream) = writer.lock() else { return };
    if let Err(error) = stream.write_all(&message.encode()) {
        eprintln!("fw-android-win: send failed: {error}");
    }
}

/// Linux evdev code of a PC keyboard key, from its set 1 scancode and the extended (E0) flag.
/// The unextended range 1..=88 is numerically identical in both tables, the extended keys are listed.
pub fn evdev_from_scancode(scancode: u16, extended: bool) -> Option<u32> {
    if !extended {
        return match scancode {
            1..=88 => Some(u32::from(scancode)),
            _ => None,
        };
    }
    Some(match scancode {
        0x1c => 96,
        0x1d => 97,
        0x35 => 98,
        0x37 => 99,
        0x38 => 100,
        0x47 => 102,
        0x48 => 103,
        0x49 => 104,
        0x4b => 105,
        0x4d => 106,
        0x4f => 107,
        0x50 => 108,
        0x51 => 109,
        0x52 => 110,
        0x53 => 111,
        0x5b => 125,
        0x5c => 126,
        0x5d => 127,
        _ => return None,
    })
}

fn evdev_from_message(wparam: WPARAM, lparam: LPARAM) -> Option<u32> {
    let flags = (lparam.0 >> 16) as u32;
    let mut scancode = (flags & 0xff) as u16;
    let mut extended = flags & 0x100 != 0;
    if scancode == 0 {
        // Synthesised input carries no scancode: derive it from the virtual key.
        // SAFETY: MapVirtualKeyW is a pure lookup.
        let mapped = unsafe { MapVirtualKeyW(wparam.0 as u32, MAPVK_VK_TO_VSC_EX) };
        scancode = (mapped & 0xff) as u16;
        extended = mapped & 0xff00 == 0xe000;
    }
    evdev_from_scancode(scancode, extended)
}

fn client_rect_size(hwnd: HWND) -> (u32, u32) {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a live RECT.
    unsafe { GetClientRect(hwnd, &mut rect) }.ok();
    ((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32)
}

fn make_bitmap(hwnd: HWND, width: u32, height: u32) -> Option<(HDC, HBITMAP, HGDIOBJ, *mut u8)> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative height makes the section top-down, the orientation of the frames.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: GDI object creation; every handle is released by `drop_bitmap` or on the failure paths below.
    unsafe {
        let window_dc = GetDC(Some(hwnd));
        let memory_dc = CreateCompatibleDC(Some(window_dc));
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(window_dc), &info, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(Some(hwnd), window_dc);
        let Ok(bitmap) = bitmap else {
            let _ = DeleteDC(memory_dc);
            return None;
        };
        let original = SelectObject(memory_dc, bitmap.into());
        Some((memory_dc, bitmap, original, bits.cast()))
    }
}

fn drop_bitmap(surface: &Surface) {
    // SAFETY: the handles were created by `make_bitmap` and are released once.
    unsafe {
        SelectObject(surface.memory_dc, surface.original);
        let _ = DeleteObject(surface.bitmap.into());
        let _ = DeleteDC(surface.memory_dc);
    }
}

fn open_window(id: u32, title: &str, width: u32, height: u32) {
    let mut rect = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
    let style = WS_OVERLAPPEDWINDOW;
    // SAFETY: plain window creation on the UI thread; the title buffer outlives the call.
    unsafe {
        AdjustWindowRectEx(&mut rect, style, false, WINDOW_EX_STYLE(0)).ok();
        let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let instance = GetModuleHandleW(None).unwrap_or_default();
        PROGRAMMATIC_RESIZE.set(true);
        let created = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WINDOW_CLASS,
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        );
        PROGRAMMATIC_RESIZE.set(false);
        let Ok(hwnd) = created else {
            eprintln!("fw-android-win: CreateWindowExW failed for window {id}");
            return;
        };
        let Some((memory_dc, bitmap, original, bits)) = make_bitmap(hwnd, width, height) else {
            let _ = DestroyWindow(hwnd);
            return;
        };
        let surface = Surface { hwnd, width, height, memory_dc, bitmap, original, bits, buttons: 0 };
        with_state(|state| {
            state.by_window.insert(hwnd.0 as isize, id);
            state.surfaces.insert(id, surface);
        });
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    }
}

fn apply_frame(window: u32, x: u32, y: u32, width: u32, height: u32, pixels: &[u8]) {
    with_state(|state| {
        let Some(surface) = state.surfaces.get(&window) else { return };
        if x >= surface.width || y >= surface.height {
            return;
        }
        let columns = width.min(surface.width - x) as usize;
        let rows = height.min(surface.height - y) as usize;
        let stride = surface.width as usize * 4;
        for row in 0..rows {
            let source = &pixels[row * width as usize * 4..][..columns * 4];
            // SAFETY: the destination row lies inside the DIB section: x + columns <= width and y + row < height.
            unsafe {
                let destination = surface.bits.add((y as usize + row) * stride + x as usize * 4);
                std::ptr::copy_nonoverlapping(source.as_ptr(), destination, columns * 4);
            }
        }
        let area = RECT { left: x as i32, top: y as i32, right: (x as usize + columns) as i32, bottom: (y as usize + rows) as i32 };
        // SAFETY: invalidating a live window region.
        let _ = unsafe { InvalidateRect(Some(surface.hwnd), Some(&area), false) };
    });
}

fn resize_surface(window: u32, width: u32, height: u32) {
    let Some(hwnd) = with_state(|state| state.surfaces.get(&window).map(|surface| surface.hwnd)).flatten() else { return };
    let Some((memory_dc, bitmap, original, bits)) = make_bitmap(hwnd, width, height) else { return };
    with_state(|state| {
        if let Some(surface) = state.surfaces.get_mut(&window) {
            drop_bitmap(surface);
            (surface.memory_dc, surface.bitmap, surface.original, surface.bits) = (memory_dc, bitmap, original, bits);
            (surface.width, surface.height) = (width, height);
        }
    });
    let mut rect = RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 };
    // SAFETY: window geometry calls on the UI thread; the programmatic flag keeps WM_SIZE from echoing.
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
        AdjustWindowRectEx(&mut rect, style, false, WINDOW_EX_STYLE(0)).ok();
        PROGRAMMATIC_RESIZE.set(true);
        SetWindowPos(hwnd, None, 0, 0, rect.right - rect.left, rect.bottom - rect.top, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE).ok();
        PROGRAMMATIC_RESIZE.set(false);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn handle(message: Message) {
    match message {
        Message::WindowOpen { window, title, width, height, .. } => open_window(window, &title, width, height),
        Message::WindowTitle { window, title } => {
            if let Some(hwnd) = with_state(|state| state.surfaces.get(&window).map(|surface| surface.hwnd)).flatten() {
                let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
                // SAFETY: NUL-terminated buffer alive for the call.
                unsafe { SetWindowTextW(hwnd, PCWSTR(wide.as_ptr())) }.ok();
            }
        }
        Message::WindowSize { window, width, height } => resize_surface(window, width, height),
        Message::Frame { window, x, y, width, height, pixels } => apply_frame(window, x, y, width, height, &pixels),
        Message::WindowClose { window } => {
            let hwnd = with_state(|state| state.surfaces.get(&window).map(|surface| surface.hwnd)).flatten();
            if let Some(hwnd) = hwnd {
                // SAFETY: destroying a window owned by this thread; WM_DESTROY releases the surface.
                unsafe { DestroyWindow(hwnd) }.ok();
            }
        }
        Message::Pointer { .. } | Message::Key { .. } | Message::ResizeRequest { .. } | Message::CloseRequest { .. } => {}
    }
}

fn drain_inbox() {
    let Some(inbox) = with_state(|state| state.inbox.clone()) else { return };
    loop {
        let event = inbox.queue.lock().ok().and_then(|mut queue| queue.pop_front());
        match event {
            Some(Event::Message(message)) => handle(message),
            Some(Event::Closed(reason)) => {
                eprintln!("fw-android-win: stream closed: {reason}");
                with_state(|state| state.exit_code = 1);
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(1) };
            }
            None => break,
        }
    }
}

fn pointer_event(hwnd: HWND, lparam: LPARAM, change: Option<(u8, bool)>) {
    let x = i32::from(lparam.0 as i16);
    let y = i32::from((lparam.0 >> 16) as i16);
    let update = with_state(|state| {
        let window = *state.by_window.get(&(hwnd.0 as isize))?;
        let surface = state.surfaces.get_mut(&window)?;
        if let Some((bit, down)) = change {
            if down {
                surface.buttons |= bit;
            } else {
                surface.buttons &= !bit;
            }
        }
        Some((window, surface.buttons))
    })
    .flatten();
    if let Some((window, buttons)) = update {
        send(&Message::Pointer { window, x, y, buttons });
    }
}

unsafe extern "system" fn window_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: every call below is a Win32 call on this window's own thread with arguments taken from the message.
    unsafe {
        let window = with_state(|state| state.by_window.get(&(hwnd.0 as isize)).copied()).flatten();
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut paint);
                let dc = with_state(|state| window.and_then(|id| state.surfaces.get(&id)).map(|surface| surface.memory_dc)).flatten();
                if let Some(dc) = dc {
                    let area = paint.rcPaint;
                    BitBlt(
                        hdc,
                        area.left,
                        area.top,
                        area.right - area.left,
                        area.bottom - area.top,
                        Some(dc),
                        area.left,
                        area.top,
                        SRCCOPY,
                    )
                    .ok();
                }
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_CLOSE => {
                if let Some(window) = window {
                    send(&Message::CloseRequest { window });
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let removed = with_state(|state| {
                    state.by_window.remove(&(hwnd.0 as isize));
                    window.and_then(|id| state.surfaces.remove(&id))
                })
                .flatten();
                if let Some(surface) = removed {
                    drop_bitmap(&surface);
                }
                LRESULT(0)
            }
            WM_ENTERSIZEMOVE => {
                SIZE_MOVE.set(true);
                LRESULT(0)
            }
            WM_EXITSIZEMOVE => {
                SIZE_MOVE.set(false);
                request_client_size(hwnd, window);
                LRESULT(0)
            }
            WM_SIZE => {
                if !PROGRAMMATIC_RESIZE.get() && !SIZE_MOVE.get() && wparam.0 as u32 != SIZE_MINIMIZED {
                    request_client_size(hwnd, window);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                pointer_event(hwnd, lparam, None);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                let (bit, down) = match message {
                    WM_LBUTTONDOWN => (BUTTON_LEFT, true),
                    WM_RBUTTONDOWN => (BUTTON_RIGHT, true),
                    WM_MBUTTONDOWN => (BUTTON_MIDDLE, true),
                    WM_LBUTTONUP => (BUTTON_LEFT, false),
                    WM_RBUTTONUP => (BUTTON_RIGHT, false),
                    _ => (BUTTON_MIDDLE, false),
                };
                if down {
                    SetCapture(hwnd);
                }
                pointer_event(hwnd, lparam, Some((bit, down)));
                if !down
                    && with_state(|state| window.and_then(|id| state.surfaces.get(&id)).map(|surface| surface.buttons)).flatten() == Some(0)
                {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
                let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
                if let (Some(window), Some(code)) = (window, evdev_from_message(wparam, lparam)) {
                    send(&Message::Key { window, code, down });
                }
                if matches!(message, WM_SYSKEYDOWN | WM_SYSKEYUP) && VIRTUAL_KEY(wparam.0 as u16) != VIRTUAL_KEY(0x73) {
                    // Alt combinations belong to the guest; only Alt+F4 keeps its default (it closes through WM_CLOSE).
                    return LRESULT(0);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn request_client_size(hwnd: HWND, window: Option<u32>) {
    let Some(window) = window else { return };
    let (width, height) = client_rect_size(hwnd);
    let known = with_state(|state| state.surfaces.get(&window).map(|surface| (surface.width, surface.height))).flatten();
    if width > 0 && height > 0 && known != Some((width, height)) {
        send(&Message::ResizeRequest { window, width, height });
    }
}

unsafe extern "system" fn sink_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WAKE {
        drain_inbox();
        return LRESULT(0);
    }
    // SAFETY: default handling of a message this window does not own.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn register_classes() -> windows::core::Result<()> {
    // SAFETY: class registration with static strings and procedures.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let window_class = WNDCLASSW {
            lpszClassName: WINDOW_CLASS,
            lpfnWndProc: Some(window_procedure),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&window_class);
        let sink_class =
            WNDCLASSW { lpszClassName: SINK_CLASS, lpfnWndProc: Some(sink_procedure), hInstance: instance.into(), ..Default::default() };
        RegisterClassW(&sink_class);
    }
    Ok(())
}

fn reader(mut stream: TcpStream, inbox: Arc<Inbox>) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => return inbox.push(Event::Closed("end of stream".into())),
            Err(error) => return inbox.push(Event::Closed(error.to_string())),
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
        }
        let mut offset = 0;
        loop {
            match Message::decode(&buffer[offset..]) {
                Ok((message, used)) => {
                    inbox.push(Event::Message(message));
                    offset += used;
                }
                Err(DecodeError::Incomplete) => break,
                Err(error) => return inbox.push(Event::Closed(error.to_string())),
            }
        }
        buffer.drain(..offset);
    }
}

/// Runs the bridge on an established stream until it closes. `ready` receives the UI thread id once windows can be driven.
pub fn run_stream(stream: TcpStream, ready: Option<Sender<u32>>) -> Result<i32, String> {
    stream.set_nodelay(true).ok();
    let read_half = stream.try_clone().map_err(|error| error.to_string())?;
    // SAFETY: process-wide DPI mode set once before any window exists; failure (already set) is harmless.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.ok();
    register_classes().map_err(|error| format!("register window classes: {error}"))?;
    let inbox = Arc::new(Inbox { queue: Mutex::new(VecDeque::new()), sink: Mutex::new(0) });
    STATE.set(Some(State {
        surfaces: HashMap::new(),
        by_window: HashMap::new(),
        writer: Arc::new(Mutex::new(stream)),
        inbox: inbox.clone(),
        exit_code: 0,
    }));
    // SAFETY: message-only sink window plus the standard message loop of this thread.
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|error| error.to_string())?;
        let sink = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            SINK_CLASS,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|error| format!("create sink window: {error}"))?;
        if let Ok(mut slot) = inbox.sink.lock() {
            *slot = sink.0 as isize;
        }
        let reader_inbox = inbox.clone();
        std::thread::spawn(move || reader(read_half, reader_inbox));
        if let Some(ready) = ready {
            ready.send(GetCurrentThreadId()).ok();
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let windows: Vec<HWND> = with_state(|state| state.surfaces.values().map(|surface| surface.hwnd).collect()).unwrap_or_default();
        for hwnd in windows {
            let _ = DestroyWindow(hwnd);
        }
        let _ = DestroyWindow(sink);
    }
    Ok(with_state(|state| state.exit_code).unwrap_or(0))
}

pub fn run(address: &str) -> Result<i32, String> {
    let stream = TcpStream::connect(address).map_err(|error| format!("connect {address}: {error}"))?;
    run_stream(stream, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancodes_map_to_evdev() {
        assert_eq!(evdev_from_scancode(0x01, false), Some(1), "Escape");
        assert_eq!(evdev_from_scancode(0x1e, false), Some(30), "A");
        assert_eq!(evdev_from_scancode(0x39, false), Some(57), "Space");
        assert_eq!(evdev_from_scancode(0x58, false), Some(88), "F12");
        assert_eq!(evdev_from_scancode(0x4b, true), Some(105), "Left");
        assert_eq!(evdev_from_scancode(0x1d, true), Some(97), "Right Ctrl");
        assert_eq!(evdev_from_scancode(0x5b, true), Some(125), "Left Windows");
        assert_eq!(evdev_from_scancode(0x00, false), None);
        assert_eq!(evdev_from_scancode(0x7f, true), None);
    }
}
