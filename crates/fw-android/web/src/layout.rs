//! Window placement on the canvas (device pixels): a tiling mode that picks the column count with the largest total
//! visible area, and a cascade mode. Pure geometry so it is tested natively.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Tile,
    Cascade,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "tile" => Some(Self::Tile),
            "cascade" => Some(Self::Cascade),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub window: u32,
    pub title: Rect,
    pub content: Rect,
    /// Content pixels on screen per guest pixel (at most 1.0, windows are never enlarged).
    pub scale: f32,
    /// Largest content size in device pixels that fits this window's slot, what a resize request asks the guest for.
    pub available: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    Title,
    Close,
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub window: u32,
    pub region: Region,
    /// Position inside the window content in guest pixels (can lie outside the window while a pointer is captured).
    pub x: f32,
    pub y: f32,
}

pub struct Metrics {
    pub title_height: f32,
    pub gap: f32,
}

fn fit(width: f32, height: f32, cell_width: f32, cell_height: f32) -> f32 {
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    (cell_width / width).min(cell_height / height).clamp(0.0, 1.0)
}

fn placed(window: u32, size: (u32, u32), left: f32, top: f32, scale: f32, available: (f32, f32), metrics: &Metrics) -> Placement {
    let width = (size.0 as f32 * scale).floor().max(1.0);
    let height = (size.1 as f32 * scale).floor().max(1.0);
    let left = left.round();
    let top = top.round();
    Placement {
        window,
        title: Rect { x: left, y: top, width, height: metrics.title_height },
        content: Rect { x: left, y: top + metrics.title_height, width, height },
        scale,
        available,
    }
}

/// Places `windows` (id, guest size) in draw order; later entries are drawn on top.
pub fn place(mode: Mode, canvas: (f32, f32), windows: &[(u32, (u32, u32))], metrics: &Metrics) -> Vec<Placement> {
    let count = windows.len();
    if count == 0 {
        return Vec::new();
    }
    match mode {
        Mode::Tile => {
            let mut best: Option<(f32, usize)> = None;
            for columns in 1..=count {
                let rows = count.div_ceil(columns);
                let cell_width = (canvas.0 - metrics.gap * (columns as f32 + 1.0)) / columns as f32;
                let cell_height = (canvas.1 - metrics.gap * (rows as f32 + 1.0)) / rows as f32 - metrics.title_height;
                let area: f32 = windows
                    .iter()
                    .map(|(_, (width, height))| {
                        let scale = fit(*width as f32, *height as f32, cell_width, cell_height);
                        scale * scale * *width as f32 * *height as f32
                    })
                    .sum();
                if best.is_none_or(|(score, _)| area > score) {
                    best = Some((area, columns));
                }
            }
            let columns = best.map_or(1, |(_, columns)| columns);
            let rows = count.div_ceil(columns);
            let cell_width = (canvas.0 - metrics.gap * (columns as f32 + 1.0)) / columns as f32;
            let cell_height = (canvas.1 - metrics.gap * (rows as f32 + 1.0)) / rows as f32;
            windows
                .iter()
                .enumerate()
                .map(|(index, (window, size))| {
                    let (column, row) = (index % columns, index / columns);
                    let scale = fit(size.0 as f32, size.1 as f32, cell_width, cell_height - metrics.title_height);
                    let outer_width = size.0 as f32 * scale;
                    let outer_height = size.1 as f32 * scale + metrics.title_height;
                    let left = metrics.gap + column as f32 * (cell_width + metrics.gap) + (cell_width - outer_width) / 2.0;
                    let top = metrics.gap + row as f32 * (cell_height + metrics.gap) + (cell_height - outer_height) / 2.0;
                    placed(*window, *size, left, top, scale, (cell_width, cell_height - metrics.title_height), metrics)
                })
                .collect()
        }
        Mode::Cascade => {
            let step = metrics.title_height + metrics.gap;
            windows
                .iter()
                .enumerate()
                .map(|(index, (window, size))| {
                    let offset = metrics.gap + (index as f32 * step) % (canvas.1 / 2.0).max(step);
                    let available = (canvas.0 - offset - metrics.gap, canvas.1 - offset - metrics.gap - metrics.title_height);
                    let scale = fit(size.0 as f32, size.1 as f32, available.0, available.1);
                    placed(*window, *size, offset, offset, scale, available, metrics)
                })
                .collect()
        }
    }
}

/// Topmost window under the point (device pixels); `placements` is in draw order.
pub fn hit_test(placements: &[Placement], x: f32, y: f32, metrics: &Metrics) -> Option<Hit> {
    placements.iter().rev().find_map(|placement| {
        let to_guest = |rect: &Rect| ((x - rect.x) / placement.scale, (y - rect.y) / placement.scale);
        if placement.title.contains(x, y) {
            let close = placement.title.x + placement.title.width - metrics.title_height;
            let region = if x >= close { Region::Close } else { Region::Title };
            Some(Hit { window: placement.window, region, x: x - placement.title.x, y: y - placement.title.y })
        } else if placement.content.contains(x, y) {
            let (guest_x, guest_y) = to_guest(&placement.content);
            Some(Hit { window: placement.window, region: Region::Content, x: guest_x, y: guest_y })
        } else {
            None
        }
    })
}

/// Content position of `placement` for a pointer at (x, y) even when the pointer left the window (capture).
pub fn to_guest(placement: &Placement, x: f32, y: f32) -> (f32, f32) {
    ((x - placement.content.x) / placement.scale, (y - placement.content.y) / placement.scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const METRICS: Metrics = Metrics { title_height: 28.0, gap: 12.0 };

    fn assert_inside(canvas: (f32, f32), placement: &Placement) {
        for rect in [placement.title, placement.content] {
            assert!(rect.x >= 0.0 && rect.y >= 0.0, "{rect:?}");
            assert!(rect.x + rect.width <= canvas.0 && rect.y + rect.height <= canvas.1, "{rect:?} on {canvas:?}");
        }
    }

    #[test]
    fn one_window_keeps_its_size_and_is_centered() {
        let placements = place(Mode::Tile, (1600.0, 900.0), &[(1, (640, 480))], &METRICS);
        assert_eq!(placements[0].scale, 1.0);
        assert_eq!(placements[0].content.width, 640.0);
        let left = placements[0].content.x;
        assert!((left - (1600.0 - 640.0) / 2.0).abs() <= 1.0);
    }

    #[test]
    fn many_windows_never_overlap_and_stay_on_the_canvas() {
        let canvas = (1280.0, 720.0);
        let windows: Vec<_> = (0..7).map(|id| (id, (1080, 1920))).collect();
        let placements = place(Mode::Tile, canvas, &windows, &METRICS);
        for (index, first) in placements.iter().enumerate() {
            assert_inside(canvas, first);
            assert!(first.scale <= 1.0 && first.scale > 0.0);
            for second in &placements[index + 1..] {
                let overlap = first.content.x < second.content.x + second.content.width
                    && second.content.x < first.content.x + first.content.width
                    && first.title.y < second.content.y + second.content.height
                    && second.title.y < first.content.y + first.content.height;
                assert!(!overlap, "{first:?} overlaps {second:?}");
            }
        }
    }

    #[test]
    fn cascade_offsets_each_window_and_hit_test_finds_the_topmost() {
        let placements = place(Mode::Cascade, (1280.0, 720.0), &[(1, (400, 300)), (2, (400, 300))], &METRICS);
        assert!(placements[1].title.x > placements[0].title.x);
        let point = (placements[1].content.x + 5.0, placements[1].content.y + 5.0);
        let hit = hit_test(&placements, point.0, point.1, &METRICS).expect("hit");
        assert_eq!((hit.window, hit.region), (2, Region::Content));
        assert_eq!((hit.x, hit.y), (5.0, 5.0));
    }

    #[test]
    fn close_button_is_the_right_end_of_the_title_bar() {
        let placements = place(Mode::Tile, (1280.0, 720.0), &[(9, (640, 480))], &METRICS);
        let title = placements[0].title;
        let close = hit_test(&placements, title.x + title.width - 4.0, title.y + 4.0, &METRICS).expect("hit");
        assert_eq!(close.region, Region::Close);
        let bar = hit_test(&placements, title.x + 4.0, title.y + 4.0, &METRICS).expect("hit");
        assert_eq!(bar.region, Region::Title);
        assert!(hit_test(&placements, 0.0, 0.0, &METRICS).is_none());
    }
}
