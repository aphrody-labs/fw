use std::{fs::File, io::BufWriter, path::Path};

use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Point, Size},
    wayland::compositor::{TraversalAction, with_states, with_surface_tree_upward},
};

use crate::{
    state::{BACKGROUND, State, Window},
    surface::{self, SurfaceImageSlot},
};

const BORDER: u32 = 0xFF5A_5D63;

pub struct Canvas {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

fn over(source: u32, destination: u32) -> u32 {
    let alpha = source >> 24;
    match alpha {
        255 => source,
        0 => destination,
        _ => {
            let inverse = 255 - alpha;
            let channel = |shift: u32| {
                let blended = ((source >> shift) & 0xFF) + ((destination >> shift) & 0xFF) * inverse / 255;
                blended.min(255) << shift
            };
            channel(24) | channel(16) | channel(8) | channel(0)
        }
    }
}

impl Canvas {
    fn fill_rect(&mut self, left: i32, top: i32, right: i32, bottom: i32, color: u32) {
        for y in top.max(0)..bottom.min(self.height) {
            for x in left.max(0)..right.min(self.width) {
                self.pixels[(y * self.width + x) as usize] = color;
            }
        }
    }

    fn blit(&mut self, image: &surface::SurfaceImage, left: i32, top: i32, right: i32, bottom: i32, source: [f64; 4]) {
        let (dest_width, dest_height) = (f64::from(right - left), f64::from(bottom - top));
        if dest_width <= 0.0 || dest_height <= 0.0 {
            return;
        }
        let columns: Vec<usize> = (left.max(0)..right.min(self.width))
            .map(|x| {
                let sx = source[0] + (f64::from(x - left) + 0.5) / dest_width * source[2];
                (sx as i32).clamp(0, image.width - 1) as usize
            })
            .collect();
        for y in top.max(0)..bottom.min(self.height) {
            let sy = source[1] + (f64::from(y - top) + 0.5) / dest_height * source[3];
            let row = (sy as i32).clamp(0, image.height - 1) as usize * image.width as usize;
            let line = (y * self.width) as usize + left.max(0) as usize;
            for (offset, column) in columns.iter().enumerate() {
                let target = &mut self.pixels[line + offset];
                *target = over(image.pixels[row + column], *target);
            }
        }
    }
}

fn physical(logical: i32, scale: f64) -> i32 {
    (f64::from(logical) * scale).round() as i32
}

fn draw_tree(canvas: &mut Canvas, root: &WlSurface, origin: Point<i32, Logical>, scale: f64) {
    with_surface_tree_upward(
        root,
        origin,
        |_, states, parent: &Point<i32, Logical>| {
            TraversalAction::DoChildren(*parent + surface::subsurface_offset(states))
        },
        |_, states, parent| {
            let origin = *parent + surface::subsurface_offset(states);
            let Some(placement) = surface::placement(states, origin) else { return };
            let Some(slot) = states.data_map.get::<SurfaceImageSlot>() else { return };
            canvas.blit(
                &slot.borrow(),
                physical(origin.x, scale),
                physical(origin.y, scale),
                physical(origin.x + placement.size.w, scale),
                physical(origin.y + placement.size.h, scale),
                placement.source,
            );
        },
        |_, _, _| true,
    );
}

fn root_size(window: &Window) -> Option<Size<i32, Logical>> {
    with_states(window.surface(), |states| {
        surface::placement(states, Point::from((0, 0))).map(|placement| placement.size)
    })
}

fn draw_window(canvas: &mut Canvas, window: &Window, scale: f64) -> bool {
    let Some(size) = root_size(window) else { return false };
    let origin = window.position;
    canvas.fill_rect(
        physical(origin.x, scale) - 1,
        physical(origin.y, scale) - 1,
        physical(origin.x + size.w, scale) + 1,
        physical(origin.y + size.h, scale) + 1,
        BORDER,
    );
    draw_tree(canvas, window.surface(), origin, scale);
    true
}

/// Renders one window at its own origin, clipped to its root surface, transparent where nothing is drawn.
pub fn render_window(window: &Window, scale: f64) -> Option<Canvas> {
    let size = root_size(window)?;
    let (width, height) = (physical(size.w, scale), physical(size.h, scale));
    let mut canvas = Canvas { width, height, pixels: vec![0; (width * height).max(0) as usize] };
    draw_tree(&mut canvas, window.surface(), Point::from((0, 0)), scale);
    Some(canvas)
}

pub fn compose(state: &State) -> (Canvas, usize) {
    let (width, height) = state.size;
    let mut canvas = Canvas { width, height, pixels: vec![BACKGROUND; (width * height).max(0) as usize] };
    let drawn = state.windows.iter().filter(|window| draw_window(&mut canvas, window, state.scale)).count();
    (canvas, drawn)
}

pub fn write_png(canvas: &Canvas, path: &Path) -> Result<(), String> {
    let file = File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), canvas.width as u32, canvas.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    let bytes: Vec<u8> = canvas
        .pixels
        .iter()
        .flat_map(|&pixel| [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8, (pixel >> 24) as u8])
        .collect();
    writer.write_image_data(&bytes).map_err(|error| error.to_string())
}
