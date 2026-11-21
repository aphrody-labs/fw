//! A fwa-stream v1 server without the Android guest: animated windows for the bridge to show, plus the self test that
//! drives real Win32 windows (input, title, resize, close) and captures them to PNG with `PrintWindow`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fw_android_stream::{DecodeError, Message};
use serde::Serialize;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::bridge;

const FRAME_INTERVAL: Duration = Duration::from_millis(33);
const SQUARE: u32 = 64;
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

struct Window {
    id: u32,
    width: u32,
    height: u32,
    open: bool,
    needs_full: bool,
    last_square: Option<(u32, u32)>,
    pointer: Option<(i32, i32)>,
}

fn digit_pixel(text: &str, scale: u32, x: u32, y: u32) -> bool {
    let (glyph_width, glyph_height) = (4 * scale, 5 * scale);
    if y >= glyph_height {
        return false;
    }
    let index = (x / glyph_width) as usize;
    let Some(character) = text.chars().nth(index).and_then(|character| character.to_digit(10)) else { return false };
    let column = (x % glyph_width) / scale;
    column < 3 && DIGITS[character as usize][(y / scale) as usize] >> (2 - column) & 1 == 1
}

/// Window 1 repaints in full every frame, the others only damage the rectangle their square moved through.
fn pixel(window: &Window, tick: u32, x: u32, y: u32) -> [u8; 4] {
    let full = window.id % 2 == 1;
    let phase = if full { tick.wrapping_mul(4) } else { 0 };
    let blue = (x * 255 / window.width.max(1)) as u8;
    let green = (y * 255 / window.height.max(1)) as u8;
    let red = ((phase + window.id * 80) % 256) as u8;
    let mut value = [blue, green, red, 255];
    if full {
        if digit_pixel(&window.id.to_string(), 6, x.wrapping_sub(16), y.wrapping_sub(16)) {
            value = [255, 255, 255, 255];
        }
        if digit_pixel(&tick.to_string(), 6, x.wrapping_sub(60), y.wrapping_sub(16)) {
            value = [255, 255, 255, 255];
        }
    }
    let (square_x, square_y) = square_position(window, tick);
    if x >= square_x && x < square_x + SQUARE && y >= square_y && y < square_y + SQUARE {
        value = [200, 60, 255, 255];
    }
    if let Some((pointer_x, pointer_y)) = window.pointer
        && (x as i32 - pointer_x).abs() <= 4
        && (y as i32 - pointer_y).abs() <= 4
    {
        value = [0, 0, 255, 255];
    }
    value
}

fn square_position(window: &Window, tick: u32) -> (u32, u32) {
    let travel = window.width.saturating_sub(SQUARE).max(1);
    (tick * 5 % travel, window.height.saturating_sub(SQUARE) / 2)
}

fn region(window: &Window, tick: u32, x: u32, y: u32, width: u32, height: u32) -> Message {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in y..y + height {
        for column in x..x + width {
            pixels.extend_from_slice(&pixel(window, tick, column, row));
        }
    }
    Message::Frame { window: window.id, x, y, width, height, pixels }
}

fn clamp(window: &Window, x: u32, y: u32, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x = x.min(window.width.saturating_sub(1));
    let y = y.min(window.height.saturating_sub(1));
    (x, y, width.min(window.width - x), height.min(window.height - y))
}

fn read_client(mut stream: TcpStream, sink: mpsc::Sender<Message>) {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(count) => buffer.extend_from_slice(&chunk[..count]),
        }
        let mut offset = 0;
        loop {
            match Message::decode(&buffer[offset..]) {
                Ok((message, used)) => {
                    offset += used;
                    if sink.send(message).is_err() {
                        return;
                    }
                }
                Err(DecodeError::Incomplete) => break,
                Err(_) => return,
            }
        }
        buffer.drain(..offset);
    }
}

pub fn title_of(window: u32) -> String {
    format!("fwa test window {window}")
}

/// Serves one connection: opens `count` windows, animates them, answers resize and close requests, and records every
/// client message in `observed`. Returns when the client disconnects or every window is closed.
fn serve_connection(mut stream: TcpStream, count: u32, observed: &Mutex<Vec<Message>>) -> std::io::Result<()> {
    stream.set_nodelay(true)?;
    let (sender, inbox): (_, Receiver<Message>) = mpsc::channel();
    let reader = stream.try_clone()?;
    std::thread::spawn(move || read_client(reader, sender));
    let mut windows: Vec<Window> = (1..=count)
        .map(|id| Window { id, width: 360 + 120 * (id % 2), height: 240, open: true, needs_full: true, last_square: None, pointer: None })
        .collect();
    for window in &windows {
        let open = Message::WindowOpen {
            window: window.id,
            app_id: "fw.android.test".into(),
            title: title_of(window.id),
            width: window.width,
            height: window.height,
        };
        stream.write_all(&open.encode())?;
    }
    let mut tick = 0u32;
    loop {
        let started = Instant::now();
        while let Ok(message) = inbox.try_recv() {
            if let Ok(mut log) = observed.lock() {
                log.push(message.clone());
            }
            let Some(window) = windows.iter_mut().find(|window| window.id == message.window()) else { continue };
            match message {
                Message::ResizeRequest { width, height, .. } => {
                    (window.width, window.height, window.needs_full) = (width, height, true);
                    stream.write_all(&Message::WindowSize { window: window.id, width, height }.encode())?;
                }
                Message::CloseRequest { .. } => {
                    window.open = false;
                    stream.write_all(&Message::WindowClose { window: window.id }.encode())?;
                }
                Message::Key { code, down: true, .. } => {
                    let title = format!("{} key {code}", title_of(window.id));
                    stream.write_all(&Message::WindowTitle { window: window.id, title }.encode())?;
                }
                Message::Pointer { x, y, .. } => window.pointer = Some((x, y)),
                _ => {}
            }
        }
        if windows.iter().all(|window| !window.open) {
            return Ok(());
        }
        for window in windows.iter_mut().filter(|window| window.open) {
            let full = window.id % 2 == 1 || window.needs_full;
            let frame = if full {
                window.needs_full = false;
                region(window, tick, 0, 0, window.width, window.height)
            } else {
                let (square_x, square_y) = square_position(window, tick);
                let (left, top) =
                    window.last_square.map_or((square_x, square_y), |previous| (previous.0.min(square_x), previous.1.min(square_y)));
                let right = window.last_square.map_or(square_x, |previous| previous.0.max(square_x)) + SQUARE;
                let (x, y, width, height) = clamp(
                    window,
                    left,
                    top,
                    right - left,
                    SQUARE + square_y.abs_diff(window.last_square.map_or(square_y, |previous| previous.1)),
                );
                window.last_square = Some((square_x, square_y));
                region(window, tick, x, y, width, height)
            };
            stream.write_all(&frame.encode())?;
        }
        tick += 1;
        std::thread::sleep(FRAME_INTERVAL.saturating_sub(started.elapsed()));
    }
}

pub fn serve(listen: &str, windows: u32) -> Result<i32, String> {
    let listener = TcpListener::bind(listen).map_err(|error| format!("bind {listen}: {error}"))?;
    println!("fwa-stream v1 test server on {}", listener.local_addr().map_err(|error| error.to_string())?);
    for connection in listener.incoming() {
        let stream = connection.map_err(|error| error.to_string())?;
        let observed = Mutex::new(Vec::new());
        match serve_connection(stream, windows, &observed) {
            Ok(()) => println!("all windows closed"),
            Err(error) => println!("client left: {error}"),
        }
        for message in observed.lock().map(|log| log.clone()).unwrap_or_default() {
            println!("{message:?}");
        }
    }
    Ok(0)
}

struct Capture {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

fn capture_with(width: u32, height: u32, fill: impl FnOnce(windows::Win32::Graphics::Gdi::HDC) -> bool) -> Option<Capture> {
    // SAFETY: GDI capture; every created object is released before returning.
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width as i32, height as i32);
        let original = SelectObject(memory, bitmap.into());
        let captured = fill(memory);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(memory, bitmap, 0, height, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        SelectObject(memory, original);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        (captured && lines != 0).then_some(Capture { width, height, bgra })
    }
}

fn window_rect(hwnd: HWND) -> Option<(i32, i32, u32, u32)> {
    let mut rect = Default::default();
    // SAFETY: `rect` is a live RECT.
    unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
    Some((rect.left, rect.top, (rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32))
}

/// The window drawn by itself: works while it is covered, minimised or on another monitor.
fn capture_window(hwnd: HWND) -> Option<Capture> {
    let (_, _, width, height) = window_rect(hwnd)?;
    // SAFETY: PrintWindow renders into the memory DC created for this capture.
    capture_with(width, height, |memory| unsafe { PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(2)) }.as_bool())
}

/// Only the screen area under the window, as the desktop compositor shows it (covers what the user would see).
fn capture_screen_area(hwnd: HWND) -> Option<Capture> {
    let (left, top, width, height) = window_rect(hwnd)?;
    capture_with(width, height, |memory| {
        // SAFETY: copies a screen rectangle into the memory DC.
        unsafe {
            let screen = GetDC(None);
            let copied = BitBlt(memory, 0, 0, width as i32, height as i32, Some(screen), left, top, SRCCOPY | CAPTUREBLT).is_ok();
            ReleaseDC(None, screen);
            copied
        }
    })
}

fn write_png(path: &Path, capture: &Capture) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), capture.width, capture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let rgba: Vec<u8> = capture.bgra.as_chunks::<4>().0.iter().flat_map(|pixel| [pixel[2], pixel[1], pixel[0], 255]).collect();
    encoder.write_header().and_then(|mut writer| writer.write_image_data(&rgba)).map_err(|error| error.to_string())
}

fn find_window(id: u32) -> Option<HWND> {
    let title: Vec<u16> = title_of(id).encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: FindWindowW with NUL-terminated strings.
    unsafe { FindWindowW(w!("FwAndroidWindow"), PCWSTR(title.as_ptr())) }.ok().filter(|hwnd| !hwnd.0.is_null())
}

fn find_window_by_prefix(id: u32) -> Option<HWND> {
    let prefix = title_of(id);
    let mut found: Option<HWND> = None;
    for hwnd in all_windows() {
        let mut buffer = [0u16; 128];
        // SAFETY: `buffer` is a live array.
        let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
        if String::from_utf16_lossy(&buffer[..length]).starts_with(&prefix) {
            found = Some(hwnd);
        }
    }
    found
}

fn all_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
        // SAFETY: lparam is the address of the Vec passed below.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        let mut class = [0u16; 32];
        // SAFETY: `class` is a live array.
        let length = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
        if String::from_utf16_lossy(&class[..length]) == "FwAndroidWindow" {
            list.push(hwnd);
        }
        true.into()
    }
    let mut list: Vec<HWND> = Vec::new();
    // SAFETY: the callback only runs during this call.
    unsafe { EnumWindows(Some(collect), LPARAM(&raw mut list as isize)) }.ok();
    list
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    condition()
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

fn lparam_point(x: i32, y: i32) -> LPARAM {
    LPARAM(((y as isize & 0xffff) << 16) | (x as isize & 0xffff))
}

fn drive(observed: &Mutex<Vec<Message>>, directory: Option<&Path>) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut check = |name: &'static str, ok: bool, detail: String| checks.push(Check { name, ok, detail });
    let seen = |predicate: &dyn Fn(&Message) -> bool| observed.lock().is_ok_and(|log| log.iter().any(predicate));
    let shot = |name: &str, capture: Option<Capture>| -> Option<String> {
        let (directory, capture) = (directory?, capture?);
        let path: PathBuf = directory.join(name);
        write_png(&path, &capture).ok().map(|()| path.display().to_string())
    };

    let opened = wait_until(Duration::from_secs(5), || find_window(1).is_some() && find_window(2).is_some());
    check("one Win32 window per WindowOpen", opened, format!("{} fwa windows", all_windows().len()));
    let (Some(first), Some(second)) = (find_window(1), find_window(2)) else { return checks };

    std::thread::sleep(Duration::from_millis(800));
    let before = capture_window(first);
    let before_screen = capture_screen_area(first);
    std::thread::sleep(Duration::from_millis(500));
    let after = capture_window(first);
    let animated = matches!((&before, &after), (Some(a), Some(b)) if a.bgra != b.bgra);
    let content = after.as_ref().is_some_and(|capture| {
        capture.bgra.as_chunks::<4>().0.iter().filter(|pixel| pixel[..3] != [0, 0, 0]).count() > capture.bgra.len() / 8
    });
    let paths = [
        shot("window1-t0.png", before),
        shot("window1-t1.png", after),
        shot("window1-screen.png", before_screen),
        shot("window2-t0.png", capture_window(second)),
    ];
    check("frames reach the window and animate", animated && content, format!("screenshots {paths:?}"));

    let (x, y) = (100, 80);
    // SAFETY: posting input messages to windows of this process.
    unsafe {
        let _ = PostMessageW(Some(first), WM_MOUSEMOVE, WPARAM(0), lparam_point(x, y));
        let _ = PostMessageW(Some(first), WM_LBUTTONDOWN, WPARAM(1), lparam_point(x, y));
        let _ = PostMessageW(Some(first), WM_LBUTTONUP, WPARAM(0), lparam_point(x, y));
        let _ = PostMessageW(Some(first), WM_KEYDOWN, WPARAM(0x41), LPARAM(0x1e << 16));
        let _ = PostMessageW(Some(first), WM_KEYUP, WPARAM(0x41), LPARAM((0x1e << 16) | (1 << 30) | (1 << 31)));
    }
    let input = wait_until(Duration::from_secs(3), || {
        seen(&|message| matches!(message, Message::Pointer { window: 1, x: 100, y: 80, buttons: 1 }))
            && seen(&|message| matches!(message, Message::Pointer { window: 1, buttons: 0, .. }))
            && seen(&|message| matches!(message, Message::Key { window: 1, code: 30, down: true }))
            && seen(&|message| matches!(message, Message::Key { window: 1, code: 30, down: false }))
    });
    check("pointer and key events reach the server as evdev", input, "Pointer{1,100,80,left} then release, Key{30} down and up".into());

    let retitled = wait_until(Duration::from_secs(3), || find_window_by_prefix(1).is_some() && find_window(1).is_none());
    check("WindowTitle updates the Win32 title", retitled, "title became \"fwa test window 1 key 30\"".into());
    let marker = wait_until(Duration::from_secs(2), || {
        capture_window(first).is_some_and(|capture| {
            let stride = capture.width as usize * 4;
            // The server paints a red marker at the pointer once it has the Pointer message; the window has a 1px frame and a caption.
            capture.bgra.as_chunks::<4>().0.iter().enumerate().any(|(index, pixel)| pixel[..3] == [0, 0, 255] && index * 4 % stride > 0)
        })
    });
    check("pointer round trip is visible in the frames", marker, "red marker drawn by the server at the click position".into());

    let target = (500, 330);
    // SAFETY: resizing a window of this process from another thread is a synchronous cross-thread call.
    unsafe { SetWindowPos(second, None, 0, 0, target.0, target.1, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) }.ok();
    let resized = wait_until(Duration::from_secs(3), || seen(&|message| matches!(message, Message::ResizeRequest { window: 2, .. })));
    let mut client = Default::default();
    // SAFETY: `client` is a live RECT.
    wait_until(Duration::from_secs(2), || {
        unsafe { GetClientRect(second, &mut client) }.ok();
        seen(
            &|message| matches!(message, Message::ResizeRequest { window: 2, width, height } if *width == (client.right - client.left) as u32 && *height == (client.bottom - client.top) as u32),
        )
    });
    std::thread::sleep(Duration::from_millis(300));
    let resized_shot = shot("window2-resized.png", capture_window(second));
    check(
        "resize sends ResizeRequest and the guest size is applied",
        resized,
        format!("client {}x{} screenshot {resized_shot:?}", client.right - client.left, client.bottom - client.top),
    );

    // SAFETY: posting close requests to windows of this process.
    unsafe {
        let _ = PostMessageW(find_window_by_prefix(1), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    let closed = wait_until(Duration::from_secs(3), || {
        seen(&|message| matches!(message, Message::CloseRequest { window: 1 })) && find_window_by_prefix(1).is_none()
    });
    check("close sends CloseRequest and WindowClose destroys the window", closed, "window 1 gone after the server answered".into());
    checks
}

pub fn self_test(directory: Option<&str>) -> Result<i32, String> {
    let directory = directory.map(PathBuf::from);
    if let Some(directory) = &directory {
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    let observed = Arc::new(Mutex::new(Vec::new()));
    let server_log = observed.clone();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            serve_connection(stream, 2, &server_log).ok();
        }
    });
    let stream = TcpStream::connect(address).map_err(|error| error.to_string())?;
    let (ready_sender, ready) = mpsc::channel();
    let driver_log = observed.clone();
    let driver = std::thread::spawn(move || {
        let Ok(thread) = ready.recv() else { return Vec::new() };
        let checks = drive(&driver_log, directory.as_deref());
        // SAFETY: ends the bridge's message loop on its own thread.
        unsafe { PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)) }.ok();
        checks
    });
    bridge::run_stream(stream, Some(ready_sender))?;
    let checks = driver.join().map_err(|_| "driver thread panicked".to_string())?;
    let failed = checks.iter().filter(|check| !check.ok).count();
    println!("{}", serde_json::to_string_pretty(&checks).map_err(|error| error.to_string())?);
    Ok(i32::from(failed != 0 || checks.is_empty()))
}
