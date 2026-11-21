//! Video to SVG in pure Rust.
//!
//! Pipeline: MP4 demux (`craft-media` isobmff) -> H.264 decode (`craft-video`, clean-room, no FFmpeg)
//! -> BT.601/BT.709 YUV 4:2:0 to RGB using the stream's VUI colour description -> frame sampling
//! at a target rate -> box downscale -> colour clustering and spline fitting (`visioncortex`, the
//! engine behind VTracer; the clustering parameters follow VTracer's "poster" preset) -> one SVG per
//! sampled frame plus one animated SVG that switches frames with SMIL `<set>` steps. `export` turns
//! the frames into Markdown, HTML, JSX and GPUI source.

pub mod export;

use craft_media::isobmff::{self, CodecConfig, TrackKind};
use craft_video::h264::decode::{ColorInfo, Decoder, Picture};
use std::fmt::Write as _;
use std::path::Path;
use visioncortex::color_clusters::{KeyingAction, Runner, RunnerConfig, HIERARCHICAL_MAX};
use visioncortex::{Color, ColorImage, PathSimplifyMode};

/// Conversion settings.
#[derive(Clone, Debug)]
pub struct Options {
    /// Sampled frames per second (frames between samples are skipped).
    pub fps: f64,
    /// Output width in pixels (height keeps the aspect ratio); `None` keeps the source width.
    pub width: Option<u32>,
    /// Colour precision in bits per channel (1-8); lower merges more colours into one shape.
    pub color_precision: i32,
    /// Smallest kept shape, in pixels of side length (area = value squared).
    pub filter_speckle: usize,
    /// Stop after this many sampled frames.
    pub max_frames: Option<usize>,
    /// Mean absolute RGB difference (0-1) to the previous sampled frame that starts a new scene.
    pub scene_threshold: f64,
}

impl Default for Options {
    fn default() -> Self {
        // Small text breaks up at 640 px whatever the speckle; at --width 1280 it stays legible (checked on
        // a 1080p clip against the decoded reference frame).
        Self { fps: 12.0, width: Some(640), color_precision: 6, filter_speckle: 2, max_frames: None, scene_threshold: 0.12 }
    }
}

/// A sampled, vectorised frame.
#[derive(Clone, Debug)]
pub struct SvgFrame {
    /// Presentation time in seconds.
    pub time: f64,
    pub width: usize,
    pub height: usize,
    /// `<path>` elements only (no `<svg>` wrapper), so frames can be embedded.
    pub paths: String,
    pub path_count: usize,
    /// Mean absolute RGB difference to the previous sampled frame (1.0 for the first frame).
    pub change: f64,
    /// First frame of a scene (first frame, or `change` above the threshold).
    pub scene_start: bool,
}

impl SvgFrame {
    /// A standalone SVG document for this frame.
    pub fn to_svg(&self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n{p}</svg>\n",
            w = self.width,
            h = self.height,
            p = self.paths
        )
    }
}

/// Errors with the stage that failed.
#[derive(Debug)]
pub enum Error {
    Io(String),
    Demux(String),
    NoVideo,
    Unsupported(String),
    Decode(String),
    Vectorize(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "read: {e}"),
            Self::Demux(e) => write!(f, "MP4 demux: {e}"),
            Self::NoVideo => write!(f, "no video track in the file"),
            Self::Unsupported(c) => write!(f, "video codec {c} is not supported (H.264 only)"),
            Self::Decode(e) => write!(f, "H.264 decode: {e}"),
            Self::Vectorize(e) => write!(f, "vectorise: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// RGB from 8-bit 4:2:0 using the matrix and range the stream declares (H.273 code points:
/// 1 = BT.709, 5/6 = BT.601; unspecified falls back to BT.709 at 720 lines and above).
pub fn picture_to_rgb(pic: &Picture) -> Vec<u8> {
    let (w, h) = (pic.width as usize, pic.height as usize);
    let bt709 = match pic.color.matrix {
        1 => true,
        5 | 6 => false,
        _ => h >= 720,
    };
    // (Kr, Kb) for each matrix.
    let (kr, kb) = if bt709 { (0.2126, 0.0722) } else { (0.299, 0.114) };
    let kg = 1.0 - kr - kb;
    let (y_off, y_scale, c_scale) = range(pic.color);
    let mut out = vec![0u8; w * h * 3];
    for row in 0..h {
        for col in 0..w {
            let yv = (f64::from(pic.y[row * pic.y_stride + col]) - y_off) * y_scale;
            let ci = (row / 2) * pic.uv_stride + col / 2;
            let cb = (f64::from(pic.u[ci]) - 128.0) * c_scale;
            let cr = (f64::from(pic.v[ci]) - 128.0) * c_scale;
            let r = yv + 2.0 * (1.0 - kr) * cr;
            let b = yv + 2.0 * (1.0 - kb) * cb;
            let g = (yv - kr * r - kb * b) / kg;
            let o = (row * w + col) * 3;
            out[o] = clamp(r);
            out[o + 1] = clamp(g);
            out[o + 2] = clamp(b);
        }
    }
    out
}

fn range(c: ColorInfo) -> (f64, f64, f64) {
    if c.full_range { (0.0, 1.0, 1.0) } else { (16.0, 255.0 / 219.0, 255.0 / 224.0) }
}

fn clamp(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Box-filter downscale of packed RGB to `dst_w` wide (aspect ratio kept). Returns (rgb, w, h).
pub fn downscale(rgb: &[u8], w: usize, h: usize, dst_w: usize) -> (Vec<u8>, usize, usize) {
    if dst_w >= w || dst_w == 0 {
        return (rgb.to_vec(), w, h);
    }
    let dst_h = ((h * dst_w) as f64 / w as f64).round().max(1.0) as usize;
    let mut out = vec![0u8; dst_w * dst_h * 3];
    for dy in 0..dst_h {
        let (y0, y1) = (dy * h / dst_h, ((dy + 1) * h / dst_h).max(dy * h / dst_h + 1));
        for dx in 0..dst_w {
            let (x0, x1) = (dx * w / dst_w, ((dx + 1) * w / dst_w).max(dx * w / dst_w + 1));
            let mut acc = [0u64; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 3;
                    acc[0] += u64::from(rgb[i]);
                    acc[1] += u64::from(rgb[i + 1]);
                    acc[2] += u64::from(rgb[i + 2]);
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            let o = (dy * dst_w + dx) * 3;
            for k in 0..3 {
                out[o + k] = (acc[k] / n) as u8;
            }
        }
    }
    (out, dst_w, dst_h)
}

/// Vectorises one RGB image into SVG `<path>` elements (stacked colour layers).
pub fn vectorize(rgb: &[u8], w: usize, h: usize, opts: &Options) -> SvgFrame {
    let mut img = ColorImage::new_w_h(w, h);
    for (i, px) in rgb.as_chunks::<3>().0.iter().enumerate() {
        img.pixels[i * 4..i * 4 + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
    }
    let runner = Runner::new(
        RunnerConfig {
            diagonal: false,
            hierarchical: HIERARCHICAL_MAX,
            batch_size: 25600,
            good_min_area: opts.filter_speckle * opts.filter_speckle,
            good_max_area: w * h,
            is_same_color_a: 8 - opts.color_precision.clamp(1, 8),
            is_same_color_b: 1,
            deepen_diff: 16,
            hollow_neighbours: 1,
            key_color: Color::default(),
            keying_action: KeyingAction::Discard,
        },
        img,
    );
    let clusters = runner.run();
    let view = clusters.view();
    let mut paths = String::new();
    let mut count = 0;
    for &index in view.clusters_output.iter().rev() {
        let cluster = view.get_cluster(index);
        let compound = cluster.to_compound_path(
            &view,
            false,
            PathSimplifyMode::Spline,
            60f64.to_radians(),
            4.0,
            10,
            45f64.to_radians(),
        );
        let (d, offset) = compound.to_svg_string(true, visioncortex::PointF64::default(), Some(2));
        if d.is_empty() {
            continue;
        }
        let _ = writeln!(
            paths,
            "<path d=\"{d}\" fill=\"{}\" transform=\"translate({},{})\"/>",
            cluster.residue_color().to_hex_string(),
            offset.x,
            offset.y
        );
        count += 1;
    }
    SvgFrame { time: 0.0, width: w, height: h, paths, path_count: count, change: 1.0, scene_start: true }
}

/// Mean absolute difference of two equally sized RGB buffers, 0 (same) to 1.
pub fn frame_change(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    a.iter().zip(b).map(|(x, y)| u64::from(x.abs_diff(*y))).sum::<u64>() as f64 / (a.len() as f64 * 255.0)
}

/// Annex-B start code.
const START: [u8; 4] = [0, 0, 0, 1];

/// Length-prefixed (avcC) NAL units to Annex-B.
fn to_annex_b(sample: &[u8], length_size: usize, out: &mut Vec<u8>) -> Result<(), Error> {
    let mut i = 0;
    while i + length_size <= sample.len() {
        let len = sample[i..i + length_size].iter().fold(0usize, |a, &b| (a << 8) | usize::from(b));
        i += length_size;
        let end = i.checked_add(len).filter(|&e| e <= sample.len()).ok_or_else(|| Error::Demux("NAL length past the sample end".into()))?;
        out.extend_from_slice(&START);
        out.extend_from_slice(&sample[i..end]);
        i = end;
    }
    Ok(())
}

/// Decodes `path`, samples frames at `opts.fps` and vectorises them, calling `on_frame` for each.
/// Returns the number of decoded pictures.
pub fn convert_file(path: &Path, opts: &Options, mut on_frame: impl FnMut(SvgFrame)) -> Result<usize, Error> {
    let bytes = std::fs::read(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    let file = isobmff::open(bytes.as_slice()).map_err(|e| Error::Demux(e.to_string()))?;
    let (ti, track) = file.tracks.iter().enumerate().find(|(_, t)| t.kind == TrackKind::Video).ok_or(Error::NoVideo)?;
    let avc = match track.codec() {
        Some(CodecConfig::Avc(c)) => c.clone(),
        Some(other) => return Err(Error::Unsupported(format!("{other:?}").chars().take(40).collect())),
        None => return Err(Error::Unsupported("unknown".into())),
    };
    let timescale = f64::from(track.timescale.max(1));
    let mut dec = Decoder::new();
    let mut header = Vec::new();
    for nal in avc.sps.iter().chain(avc.pps.iter()) {
        header.extend_from_slice(&START);
        header.extend_from_slice(nal);
    }
    let step = 1.0 / opts.fps.max(0.001);
    let mut next_time = 0.0;
    let mut emitted = 0usize;
    let mut decoded = 0usize;
    let mut previous: Option<Vec<u8>> = None;
    let mut handle = |pics: Vec<Picture>, next_time: &mut f64, emitted: &mut usize| -> bool {
        for pic in pics {
            decoded += 1;
            let t = pic.pts as f64 / timescale;
            if t + 1e-9 < *next_time {
                continue;
            }
            if opts.max_frames.is_some_and(|m| *emitted >= m) {
                return false;
            }
            let rgb = picture_to_rgb(&pic);
            let (small, w, h) = downscale(&rgb, pic.width as usize, pic.height as usize, opts.width.unwrap_or(pic.width) as usize);
            let mut frame = vectorize(&small, w, h, opts);
            frame.time = t;
            frame.change = previous.as_deref().map_or(1.0, |p| frame_change(p, &small));
            frame.scene_start = previous.is_none() || frame.change >= opts.scene_threshold;
            previous = Some(small);
            on_frame(frame);
            *emitted += 1;
            *next_time = t + step;
        }
        true
    };
    let mut au = Vec::new();
    for (si, sample) in track.samples.iter().enumerate() {
        let data = file.read_sample(bytes.as_slice(), ti, si).map_err(|e| Error::Demux(e.to_string()))?;
        au.clear();
        if si == 0 {
            au.extend_from_slice(&header);
        }
        to_annex_b(&data, usize::from(avc.length_size), &mut au)?;
        let pics = dec.decode(&au, sample.pts).map_err(|e| Error::Decode(e.to_string()))?;
        if !handle(pics, &mut next_time, &mut emitted) {
            return Ok(decoded);
        }
    }
    let rest = dec.flush();
    if let Some(e) = dec.take_error() {
        return Err(Error::Decode(e.to_string()));
    }
    handle(rest, &mut next_time, &mut emitted);
    Ok(decoded)
}

/// One SVG that shows each frame for its duration and loops, using SMIL `<set>` on visibility.
pub fn animated_svg(frames: &[SvgFrame]) -> String {
    let (w, h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let total = frames.windows(2).map(|p| p[1].time - p[0].time).sum::<f64>()
        + frames.windows(2).last().map_or(1.0, |p| p[1].time - p[0].time);
    let start = frames.first().map_or(0.0, |f| f.time);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n<rect id=\"clock\" width=\"0\" height=\"0\"><animate id=\"loop\" attributeName=\"x\" from=\"0\" to=\"0\" dur=\"{total:.3}s\" begin=\"0s;loop.end\"/></rect>\n"
    );
    for (i, f) in frames.iter().enumerate() {
        let begin = f.time - start;
        let end = frames.get(i + 1).map_or(total, |n| n.time - start);
        let _ = write!(
            out,
            "<g visibility=\"hidden\"><set attributeName=\"visibility\" to=\"visible\" begin=\"loop.begin+{begin:.3}s\" dur=\"{:.3}s\"/>\n{}</g>\n",
            (end - begin).max(0.001),
            f.paths
        );
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_keeps_aspect_and_averages() {
        let rgb: Vec<u8> = [[0u8, 0, 0], [255, 255, 255]].iter().cycle().take(4 * 2).flatten().copied().collect();
        let (out, w, h) = downscale(&rgb, 4, 2, 2);
        assert!(w == 2 && h == 1);
        assert!(out.iter().all(|&v| v > 100 && v < 160), "{out:?}");
    }

    #[test]
    fn two_colour_image_gives_paths_in_both_colours() {
        let (w, h) = (32, 32);
        let mut rgb = vec![0u8; w * h * 3];
        for y in 0..h {
            for x in w / 2..w {
                let i = (y * w + x) * 3;
                rgb[i] = 230;
            }
        }
        let f = vectorize(&rgb, w, h, &Options::default());
        assert!(f.path_count >= 2, "{}", f.paths);
        assert!(f.paths.contains("#E6") || f.paths.contains("#e6"), "{}", f.paths);
    }

    #[test]
    fn frame_change_is_zero_for_equal_and_grows_with_difference() {
        assert!(frame_change(&[10, 20, 30], &[10, 20, 30]) == 0.0);
        assert!(frame_change(&[0, 0, 0], &[255, 255, 255]) > frame_change(&[0, 0, 0], &[10, 10, 10]));
    }

    #[test]
    fn annex_b_rejects_truncated_nal() {
        let mut out = Vec::new();
        assert!(to_annex_b(&[0, 0, 0, 2, 0x65, 0x88], 4, &mut out).is_ok());
        assert_eq!(out, [0, 0, 0, 1, 0x65, 0x88]);
        assert!(to_annex_b(&[0, 0, 0, 9, 0x65], 4, &mut Vec::new()).is_err());
    }

    #[test]
    fn animated_svg_has_one_step_per_frame() {
        let frames: Vec<SvgFrame> = (0..3)
            .map(|i| SvgFrame { time: f64::from(i) * 0.5, width: 4, height: 4, paths: String::new(), path_count: 0, change: 0.0, scene_start: i == 0 })
            .collect();
        let svg = animated_svg(&frames);
        assert_eq!(svg.matches("<set ").count(), 3);
        assert!(svg.contains("dur=\"1.500s\""));
    }
}
