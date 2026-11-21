//! `?demo=1`: a stand-in compositor that speaks fwa-stream v1 through the same message path as a WebSocket server, so
//! the viewer can be verified without the Android guest. Pure Rust, tested natively.

use fw_android_stream::Message;

const MIN_SIZE: u32 = 96;
const MAX_SIZE: u32 = 4096;

#[derive(Clone, Copy)]
enum Kind {
    Gradient,
    Bouncer,
    Input,
}

struct Window {
    id: u32,
    kind: Kind,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    pointer: Option<(i32, i32, u8)>,
    keys: u32,
    last_step: f64,
    square: (f32, f32),
    bar: u32,
}

fn bgra(red: u8, green: u8, blue: u8) -> [u8; 4] {
    [blue, green, red, 0xff]
}

fn hue(position: f32) -> [u8; 4] {
    let phase = position.rem_euclid(1.0) * 6.0;
    let rise = ((phase % 1.0) * 255.0) as u8;
    let fall = 255 - rise;
    match phase as u32 {
        0 => bgra(255, rise, 0),
        1 => bgra(fall, 255, 0),
        2 => bgra(0, 255, rise),
        3 => bgra(0, fall, 255),
        4 => bgra(rise, 0, 255),
        _ => bgra(255, 0, fall),
    }
}

impl Window {
    fn new(id: u32, kind: Kind, width: u32, height: u32) -> Self {
        let mut window = Self {
            id,
            kind,
            width,
            height,
            pixels: Vec::new(),
            pointer: None,
            keys: 0,
            last_step: 0.0,
            square: (40.0, 40.0),
            bar: width / 2,
        };
        window.reallocate(width, height);
        window
    }

    fn reallocate(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.pixels = vec![0; width as usize * height as usize * 4];
        self.paint(0, 0, width, height);
    }

    fn put(&mut self, x: u32, y: u32, color: [u8; 4]) {
        let offset = (y as usize * self.width as usize + x as usize) * 4;
        if let Some(slot) = self.pixels.get_mut(offset..offset + 4) {
            slot.copy_from_slice(&color);
        }
    }

    /// Repaints a rectangle of the backing buffer from the window state; the byte order is BGRA like the protocol.
    fn paint(&mut self, x0: u32, y0: u32, width: u32, height: u32) {
        let (full_width, full_height) = (self.width, self.height);
        let (x1, y1) = ((x0 + width).min(full_width), (y0 + height).min(full_height));
        for y in y0..y1 {
            for x in x0..x1 {
                let color = match self.kind {
                    Kind::Gradient => {
                        let corner = 28;
                        let (left, top) = (x < corner, y < corner);
                        let (right, bottom) = (x >= full_width.saturating_sub(corner), y >= full_height.saturating_sub(corner));
                        if x >= self.bar && x < self.bar + 6 {
                            bgra(255, 255, 255)
                        } else if left && top {
                            bgra(255, 0, 0)
                        } else if right && top {
                            bgra(0, 255, 0)
                        } else if left && bottom {
                            bgra(0, 0, 255)
                        } else if right && bottom {
                            bgra(255, 255, 255)
                        } else {
                            let shade = 0.55 + 0.45 * (y as f32 / full_height as f32);
                            let base = hue(x as f32 / full_width as f32);
                            [(base[0] as f32 * shade) as u8, (base[1] as f32 * shade) as u8, (base[2] as f32 * shade) as u8, 0xff]
                        }
                    }
                    Kind::Bouncer => {
                        let (square_x, square_y) = (self.square.0 as u32, self.square.1 as u32);
                        let inside = x >= square_x && x < square_x + 64 && y >= square_y && y < square_y + 64;
                        let checker = ((x / 32) + (y / 32)) % 2 == 0;
                        if inside {
                            bgra(255, 196, 0)
                        } else if checker {
                            bgra(36, 40, 44)
                        } else {
                            bgra(52, 58, 64)
                        }
                    }
                    Kind::Input => {
                        let cell = (x / 40, y / 40);
                        let lit = cell.1 == 0 && cell.0 < 16 && self.keys & (1 << cell.0) != 0;
                        if lit {
                            hue(cell.0 as f32 / 16.0)
                        } else if cell.1 == 0 && cell.0 < 16 {
                            bgra(48, 52, 58)
                        } else {
                            bgra(22, 24, 28)
                        }
                    }
                };
                self.put(x, y, color);
            }
        }
        if let (Kind::Input, Some((px, py, buttons))) = (self.kind, self.pointer) {
            let color = if buttons == 0 { bgra(230, 230, 230) } else { bgra(255, 90, 60) };
            for offset in -14i32..=14 {
                for (cx, cy) in [(px + offset, py), (px, py + offset)] {
                    if cx >= x0 as i32 && cy >= y0 as i32 && (cx as u32) < x1 && (cy as u32) < y1 {
                        self.put(cx as u32, cy as u32, color);
                    }
                }
            }
        }
    }

    fn frame(&self, x: u32, y: u32, width: u32, height: u32) -> Option<Message> {
        let (x1, y1) = ((x + width).min(self.width), (y + height).min(self.height));
        if x >= x1 || y >= y1 {
            return None;
        }
        let (width, height) = (x1 - x, y1 - y);
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in y..y1 {
            let start = (row as usize * self.width as usize + x as usize) * 4;
            pixels.extend_from_slice(&self.pixels[start..start + width as usize * 4]);
        }
        Some(Message::Frame { window: self.id, x, y, width, height, pixels })
    }

    fn open(&self) -> Message {
        let (app_id, title) = match self.kind {
            Kind::Gradient => ("fw.demo.gradient", "Gradient (red, green, blue, white corners)"),
            Kind::Bouncer => ("fw.demo.bouncer", "Bouncer"),
            Kind::Input => ("fw.demo.input", "Input echo"),
        };
        Message::WindowOpen { window: self.id, app_id: app_id.into(), title: title.into(), width: self.width, height: self.height }
    }

    /// Advances the animation and returns the damage rectangle that changed.
    fn step(&mut self, now: f64) -> Option<Message> {
        let elapsed = now - self.last_step;
        match self.kind {
            Kind::Gradient => {
                let previous = self.bar;
                self.bar = ((now / 6.0) as u32) % self.width.max(1);
                let left = previous.min(self.bar);
                let span = previous.abs_diff(self.bar) + 6;
                if span > self.width / 2 {
                    self.paint(0, 0, self.width, self.height);
                    return self.frame(0, 0, self.width, self.height);
                }
                self.paint(left, 0, span, self.height);
                self.frame(left, 0, span, self.height)
            }
            Kind::Bouncer if elapsed >= 33.0 => {
                self.last_step = now;
                let before = (self.square.0 as u32, self.square.1 as u32);
                let phase = (now / 1000.0) as f32;
                let (room_x, room_y) = ((self.width.saturating_sub(64)) as f32, (self.height.saturating_sub(64)) as f32);
                let wave = |value: f32| 1.0 - (value.rem_euclid(2.0) - 1.0).abs();
                self.square = (wave(phase * 0.7) * room_x, wave(phase * 0.45 + 0.3) * room_y);
                let after = (self.square.0 as u32, self.square.1 as u32);
                let (left, top) = (before.0.min(after.0), before.1.min(after.1));
                let (width, height) = (before.0.abs_diff(after.0) + 64, before.1.abs_diff(after.1) + 64);
                self.paint(left, top, width, height);
                self.frame(left, top, width, height)
            }
            _ => None,
        }
    }
}

pub struct Demo {
    windows: Vec<Window>,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    pub fn new() -> Self {
        Self {
            windows: vec![
                Window::new(1, Kind::Gradient, 640, 400),
                Window::new(2, Kind::Bouncer, 360, 640),
                Window::new(3, Kind::Input, 640, 300),
            ],
        }
    }

    /// Messages a compositor sends right after a client connects.
    pub fn open(&self) -> Vec<Message> {
        self.windows
            .iter()
            .flat_map(|window| [window.open(), window.frame(0, 0, window.width, window.height).unwrap_or(Message::WindowClose { window: window.id })])
            .collect()
    }

    /// Animation tick, `now` is `performance.now()` in milliseconds.
    pub fn step(&mut self, now: f64) -> Vec<Message> {
        self.windows.iter_mut().filter_map(|window| window.step(now)).collect()
    }

    /// Handles a client-to-server message and returns the server messages it causes.
    pub fn input(&mut self, message: &Message) -> Vec<Message> {
        let Some(index) = self.windows.iter().position(|window| window.id == message.window()) else {
            return Vec::new();
        };
        let window = &mut self.windows[index];
        match message {
            Message::CloseRequest { .. } => {
                let window = self.windows.remove(index);
                vec![Message::WindowClose { window: window.id }]
            }
            Message::ResizeRequest { width, height, .. } => {
                let (width, height) = ((*width).clamp(MIN_SIZE, MAX_SIZE), (*height).clamp(MIN_SIZE, MAX_SIZE));
                if (width, height) == (window.width, window.height) {
                    return Vec::new();
                }
                window.reallocate(width, height);
                let mut reply = vec![Message::WindowSize { window: window.id, width, height }];
                reply.extend(window.frame(0, 0, width, height));
                reply
            }
            Message::Pointer { x, y, buttons, .. } if matches!(window.kind, Kind::Input) => {
                let previous = window.pointer.replace((*x, *y, *buttons));
                let mut damage = vec![];
                for (cx, cy) in previous.map(|(px, py, _)| (px, py)).into_iter().chain([(*x, *y)]) {
                    let left = (cx - 16).max(0) as u32;
                    let top = (cy - 16).max(0) as u32;
                    window.paint(left, top, 33, 33);
                    damage.extend(window.frame(left, top, 33, 33));
                }
                damage
            }
            Message::Key { code, down, .. } if matches!(window.kind, Kind::Input) => {
                let bit = 1u32 << (code % 16);
                if *down {
                    window.keys |= bit;
                } else {
                    window.keys &= !bit;
                }
                let left = (code % 16) * 40;
                window.paint(left, 0, 40, 40);
                window.frame(left, 0, 40, 40).into_iter().collect()
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_sends_a_window_open_and_a_full_frame_per_window_and_everything_decodes() {
        let demo = Demo::new();
        let messages = demo.open();
        assert_eq!(messages.len(), 6);
        for message in &messages {
            let bytes = message.encode();
            let (decoded, used) = Message::decode(&bytes).expect("decode");
            assert_eq!((&decoded, used), (message, bytes.len()));
        }
        let Message::Frame { x, y, width, height, pixels, .. } = &messages[1] else { panic!("frame expected") };
        assert_eq!((*x, *y, *width, *height), (0, 0, 640, 400));
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255], "top-left corner is red in BGRA byte order");
    }

    #[test]
    fn steps_emit_in_bounds_damage_and_resize_reallocates() {
        let mut demo = Demo::new();
        for tick in 1..40 {
            for message in demo.step(tick as f64 * 17.0) {
                let Message::Frame { window, x, y, width, height, pixels } = message else { panic!("frame expected") };
                let target = demo.windows.iter().find(|candidate| candidate.id == window).expect("window");
                assert!(x + width <= target.width && y + height <= target.height);
                assert_eq!(pixels.len(), (width * height * 4) as usize);
            }
        }
        let reply = demo.input(&Message::ResizeRequest { window: 1, width: 500, height: 333 });
        assert!(matches!(reply[0], Message::WindowSize { window: 1, width: 500, height: 333 }));
        assert!(matches!(reply[1], Message::Frame { width: 500, height: 333, .. }));
        assert_eq!(demo.input(&Message::CloseRequest { window: 2 }), vec![Message::WindowClose { window: 2 }]);
    }

    #[test]
    fn pointer_and_key_in_the_input_window_repaint_their_area() {
        let mut demo = Demo::new();
        assert!(!demo.input(&Message::Pointer { window: 3, x: 100, y: 100, buttons: 1 }).is_empty());
        let keyed = demo.input(&Message::Key { window: 3, code: 30, down: true });
        let Message::Frame { pixels, .. } = &keyed[0] else { panic!("frame expected") };
        assert_ne!(&pixels[0..4], &[58, 52, 48, 255], "lit key cell differs from the idle color");
    }
}
