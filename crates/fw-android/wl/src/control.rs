use std::path::Path;

use serde_json::json;
use smithay::{
    backend::input::KeyState,
    input::keyboard::{FilterResult, Keycode},
    utils::{Point, SERIAL_COUNTER},
};

use crate::{render, state::State};

pub fn execute(state: &mut State, line: &str) -> String {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("screenshot") => match words.next() {
            Some(path) => screenshot(state, Path::new(path)),
            None => "err usage: screenshot <abs path.png>".into(),
        },
        Some("windows") => windows(state),
        Some("pointer") => pointer(state, words.next(), words.next(), words.next()),
        Some("key") => key(state, words.next(), words.next()),
        Some("quit") => {
            state.quit = true;
            "ok".into()
        }
        Some(other) => format!("err unknown command {other}"),
        None => "err empty command".into(),
    }
}

fn screenshot(state: &State, path: &Path) -> String {
    let (canvas, drawn) = render::compose(state);
    match render::write_png(&canvas, path) {
        Ok(()) => format!("ok {}x{} windows={drawn}", canvas.width, canvas.height),
        Err(error) => format!("err {error}"),
    }
}

fn windows(state: &State) -> String {
    let entries: Vec<_> = state
        .windows
        .iter()
        .map(|window| {
            let (app_id, title) = window.labels();
            let size = state.window_size(window);
            json!({
                "app_id": app_id.unwrap_or_default(),
                "title": title.unwrap_or_default(),
                "w": size.map_or(0, |size| size.w),
                "h": size.map_or(0, |size| size.h),
                "x": window.position.x,
                "y": window.position.y,
                "mapped": state.is_mapped(window),
            })
        })
        .collect();
    serde_json::Value::Array(entries).to_string()
}

fn pointer(state: &mut State, x: Option<&str>, y: Option<&str>, button: Option<&str>) -> String {
    let (Some(Ok(x)), Some(Ok(y))) = (x.map(str::parse::<f64>), y.map(str::parse::<f64>)) else {
        return "err usage: pointer <x> <y> <left|none>".into();
    };
    let pressed = match button {
        Some("left") => true,
        Some("none") => false,
        _ => return "err usage: pointer <x> <y> <left|none>".into(),
    };
    state.pointer_motion(Point::from((x / state.scale, y / state.scale)));
    state.pointer_button(pressed);
    "ok".into()
}

fn key(state: &mut State, code: Option<&str>, direction: Option<&str>) -> String {
    let Some(Ok(code)) = code.map(str::parse::<u32>) else {
        return "err usage: key <linux-keycode> <down|up>".into();
    };
    let key_state = match direction {
        Some("down") => KeyState::Pressed,
        Some("up") => KeyState::Released,
        _ => return "err usage: key <linux-keycode> <down|up>".into(),
    };
    let Some(keyboard) = state.seat.get_keyboard() else {
        return "err keyboard capability unavailable".into();
    };
    let time = state.now_ms();
    keyboard.input::<(), _>(
        state,
        Keycode::new(code + 8),
        key_state,
        SERIAL_COUNTER.next_serial(),
        time,
        |_, _, _| FilterResult::Forward,
    );
    "ok".into()
}
