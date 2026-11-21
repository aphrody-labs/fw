//! Exporters from vectorised frames to documents and source code.
//!
//! All exporters reference frame files as `frames/frame-NNNNN.svg` (written by [`write_all`]) except
//! HTML, which inlines the animated SVG so the page is a single self-contained file.

use crate::{animated_svg, SvgFrame};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Output formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// `frames/*.svg` and `animated.svg`.
    Svg,
    /// `video.md`: front matter, scenes with their first frame, full frame timeline.
    Markdown,
    /// `video.html`: self-contained page with the inline animated SVG and scene buttons.
    Html,
    /// `VideoSvg.jsx`: a React component that plays the frames.
    Jsx,
    /// `video_svg.rs`: a GPUI view that plays the frames through `img(Image::from_bytes(Svg, ..))`.
    Gpui,
}

impl Format {
    pub const ALL: [Format; 5] = [Format::Svg, Format::Markdown, Format::Html, Format::Jsx, Format::Gpui];

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "svg" => Self::Svg,
            "md" | "markdown" => Self::Markdown,
            "html" => Self::Html,
            "jsx" | "react" => Self::Jsx,
            "gpui" => Self::Gpui,
            _ => return None,
        })
    }
}

/// Facts about the source shown in the documents.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub name: String,
    pub decoded_frames: usize,
}

fn frame_file(i: usize) -> String {
    format!("frame-{:05}.svg", i + 1)
}

fn duration(frames: &[SvgFrame]) -> f64 {
    match (frames.first(), frames.last()) {
        (Some(a), Some(b)) if frames.len() > 1 => b.time - a.time + (b.time - frames[frames.len() - 2].time),
        _ => 0.0,
    }
}

fn scenes(frames: &[SvgFrame]) -> Vec<usize> {
    frames.iter().enumerate().filter(|(_, f)| f.scene_start).map(|(i, _)| i).collect()
}

fn esc_md(s: &str) -> String {
    s.replace('|', "\\|").replace('[', "\\[").replace(']', "\\]")
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Markdown document.
pub fn markdown(frames: &[SvgFrame], src: &Source) -> String {
    let (w, h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let sc = scenes(frames);
    let mut md = format!(
        "---\nsource: {:?}\nduration_s: {:.3}\nwidth: {w}\nheight: {h}\ndecoded_frames: {}\nsampled_frames: {}\nscenes: {}\n---\n\n# {}\n\n",
        src.name,
        duration(frames),
        src.decoded_frames,
        frames.len(),
        sc.len(),
        esc_md(&src.name)
    );
    md.push_str("Animated: [animated.svg](animated.svg)\n\n## Scenes\n\n");
    for (n, &i) in sc.iter().enumerate() {
        let f = &frames[i];
        let end = sc.get(n + 1).map_or(duration(frames) + frames[0].time, |&j| frames[j].time);
        let _ = writeln!(md, "### Scene {} ({:.2} s to {:.2} s)\n\n![Scene {} at {:.2} s](frames/{})\n", n + 1, f.time, end, n + 1, f.time, frame_file(i));
    }
    md.push_str("## Frames\n\n| # | Time (s) | Change | Paths | File |\n|---|---|---|---|---|\n");
    for (i, f) in frames.iter().enumerate() {
        let _ = writeln!(md, "| {} | {:.3} | {:.3} | {} | [{}](frames/{}) |", i + 1, f.time, f.change, f.path_count, frame_file(i), frame_file(i));
    }
    md
}

/// Self-contained HTML page.
pub fn html(frames: &[SvgFrame], src: &Source) -> String {
    let sc = scenes(frames);
    let start = frames.first().map_or(0.0, |f| f.time);
    let mut buttons = String::new();
    for (n, &i) in sc.iter().enumerate() {
        let t = frames[i].time - start;
        let _ = write!(buttons, "<button type=\"button\" data-t=\"{t:.3}\">Scene {} ({t:.2} s)</button>", n + 1);
    }
    let anim = animated_svg(frames).replacen("<svg ", "<svg id=\"video\" role=\"img\" aria-label=\"Vectorised video\" ", 1);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>body{{margin:0;font:16px/1.5 system-ui,sans-serif;background:#111;color:#eee}}main{{max-width:1100px;margin:0 auto;padding:24px}}h1{{overflow-wrap:anywhere;font-size:clamp(20px,3vw,32px)}}svg#video{{width:100%;height:auto;display:block;border-radius:12px;background:#000}}nav{{display:flex;flex-wrap:wrap;gap:8px;margin-top:16px}}button{{font:inherit;min-height:44px;padding:0 16px;border-radius:22px;border:1px solid #555;background:#222;color:#eee;cursor:pointer}}button:focus-visible{{outline:3px solid #8ab4f8;outline-offset:2px}}</style>\n</head>\n<body>\n<main>\n<h1>{title}</h1>\n{anim}<nav aria-label=\"Scenes\"><button type=\"button\" id=\"pause\">Pause</button>{buttons}</nav>\n<p>{n} vector frames, {d:.2} s.</p>\n</main>\n<script>\nconst v=document.getElementById('video');const p=document.getElementById('pause');\np.onclick=()=>{{if(v.animationsPaused()){{v.unpauseAnimations();p.textContent='Pause'}}else{{v.pauseAnimations();p.textContent='Play'}}}};\ndocument.querySelectorAll('button[data-t]').forEach(b=>b.onclick=()=>v.setCurrentTime(parseFloat(b.dataset.t)));\n</script>\n</body>\n</html>\n",
        title = esc_html(&src.name),
        n = frames.len(),
        d = duration(frames)
    )
}

/// React component. Path markup is valid JSX as written (`d`, `fill`, `transform`).
pub fn jsx(frames: &[SvgFrame], src: &Source) -> String {
    let (w, h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let start = frames.first().map_or(0.0, |f| f.time);
    let mut out = format!(
        "// Generated by fw-video from {:?}. {} frames, {:.2} s.\nimport {{ useEffect, useState }} from \"react\";\n\nconst TIMES = [{}];\nconst DURATION = {:.3};\n\nconst FRAMES = [\n",
        src.name,
        frames.len(),
        duration(frames),
        frames.iter().map(|f| format!("{:.3}", f.time - start)).collect::<Vec<_>>().join(", "),
        duration(frames)
    );
    for f in frames {
        let _ = writeln!(out, "  <g key=\"{:.3}\">{}</g>,", f.time, f.paths.replace('\n', ""));
    }
    let _ = write!(
        out,
        "];\n\nexport default function VideoSvg({{ playing = true, ...props }}) {{\n  const [index, setIndex] = useState(0);\n  useEffect(() => {{\n    if (!playing) return undefined;\n    let raf = 0;\n    const t0 = performance.now();\n    const tick = (now) => {{\n      const t = ((now - t0) / 1000) % DURATION;\n      let i = 0;\n      while (i + 1 < TIMES.length && TIMES[i + 1] <= t) i += 1;\n      setIndex(i);\n      raf = requestAnimationFrame(tick);\n    }};\n    raf = requestAnimationFrame(tick);\n    return () => cancelAnimationFrame(raf);\n  }}, [playing]);\n  return (\n    <svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Vectorised video\" {{...props}}>\n      {{FRAMES[index]}}\n    </svg>\n  );\n}}\n"
    );
    out
}

/// GPUI view (Rust). Frames are embedded with `include_bytes!` from `frames/`, relative to this file.
pub fn gpui(frames: &[SvgFrame], src: &Source) -> String {
    let start = frames.first().map_or(0.0, |f| f.time);
    let mut list = String::new();
    for (i, f) in frames.iter().enumerate() {
        let _ = writeln!(list, "    ({:.3}, include_bytes!(\"frames/{}\")),", f.time - start, frame_file(i));
    }
    format!(
        "//! Generated by fw-video from {name:?}: {n} vector frames, {d:.2} s.\n//! `VideoSvgView` plays them in a loop; each frame renders in colour through\n//! `img(Image::from_bytes(ImageFormat::Svg, ..))` (resvg), not the monochrome `svg()` icon element.\n\nuse gpui::{{img, Context, Image, ImageFormat, IntoElement, ParentElement, Render, Styled, Window, div}};\nuse std::sync::Arc;\nuse std::time::{{Duration, Instant}};\n\n/// (seconds from start, SVG bytes) for every frame, in order.\npub const FRAMES: &[(f64, &[u8])] = &[\n{list}];\n\n/// Loop length in seconds.\npub const DURATION: f64 = {d:.3};\n\npub struct VideoSvgView {{\n    images: Vec<Arc<Image>>,\n    started: Instant,\n    pub playing: bool,\n}}\n\nimpl VideoSvgView {{\n    pub fn new() -> Self {{\n        let images = FRAMES.iter().map(|(_, bytes)| Arc::new(Image::from_bytes(ImageFormat::Svg, bytes.to_vec()))).collect();\n        Self {{ images, started: Instant::now(), playing: true }}\n    }}\n\n    /// Index of the frame shown at `elapsed`.\n    pub fn frame_at(elapsed: Duration) -> usize {{\n        let t = elapsed.as_secs_f64() % DURATION.max(f64::EPSILON);\n        FRAMES.iter().rposition(|(start, _)| *start <= t).unwrap_or(0)\n    }}\n}}\n\nimpl Default for VideoSvgView {{\n    fn default() -> Self {{\n        Self::new()\n    }}\n}}\n\nimpl Render for VideoSvgView {{\n    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {{\n        if self.playing {{\n            window.request_animation_frame();\n        }}\n        let index = Self::frame_at(self.started.elapsed());\n        div().size_full().child(img(self.images[index].clone()).size_full())\n    }}\n}}\n",
        name = src.name,
        n = frames.len(),
        d = duration(frames)
    )
}

/// Writes the requested formats into `dir`; returns the written paths.
pub fn write_all(dir: &Path, frames: &[SvgFrame], src: &Source, formats: &[Format]) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    let mut put = |rel: &str, body: String| -> std::io::Result<()> {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, body)?;
        written.push(p);
        Ok(())
    };
    let needs_frames = formats.iter().any(|f| matches!(f, Format::Svg | Format::Markdown | Format::Gpui));
    if needs_frames {
        for (i, f) in frames.iter().enumerate() {
            put(&format!("frames/{}", frame_file(i)), f.to_svg())?;
        }
    }
    for format in formats {
        match format {
            Format::Svg => put("animated.svg", animated_svg(frames))?,
            Format::Markdown => {
                put("animated.svg", animated_svg(frames))?;
                put("video.md", markdown(frames, src))?;
            }
            Format::Html => put("video.html", html(frames, src))?,
            Format::Jsx => put("VideoSvg.jsx", jsx(frames, src))?,
            Format::Gpui => put("video_svg.rs", gpui(frames, src))?,
        }
    }
    written.sort();
    written.dedup();
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<SvgFrame> {
        (0..4)
            .map(|i| SvgFrame {
                time: 1.0 + f64::from(i) * 0.5,
                width: 8,
                height: 4,
                paths: format!("<path d=\"M0 0L{i} 1Z\" fill=\"#AA0000\" transform=\"translate(0,0)\"/>\n"),
                path_count: 1,
                change: if i == 2 { 0.5 } else { 0.01 },
                scene_start: i == 0 || i == 2,
            })
            .collect()
    }

    #[test]
    fn markdown_lists_each_scene_with_its_first_frame() {
        let md = markdown(&sample(), &Source { name: "clip.mp4".into(), decoded_frames: 12 });
        assert_eq!(md.matches("### Scene ").count(), 2);
        assert!(md.contains("frames/frame-00003.svg"));
        assert_eq!(md.matches("\n| ").count(), 4 + 1);
    }

    #[test]
    fn html_is_self_contained_with_scene_buttons() {
        let page = html(&sample(), &Source { name: "<clip>".into(), decoded_frames: 4 });
        assert!(page.contains("&lt;clip&gt;") && !page.contains("<clip>"));
        assert!(page.contains("<svg id=\"video\""));
        assert_eq!(page.matches("data-t=").count(), 2);
        assert!(!page.contains("frames/"));
    }

    #[test]
    fn jsx_and_gpui_reference_every_frame_in_order() {
        let f = sample();
        let j = jsx(&f, &Source::default());
        assert_eq!(j.matches("<g key=").count(), 4);
        assert!(j.contains("const TIMES = [0.000, 0.500, 1.000, 1.500]"));
        let g = gpui(&f, &Source::default());
        let first = g.find("frame-00001.svg").unwrap();
        let last = g.find("frame-00004.svg").unwrap();
        assert!(first < last);
        assert!(g.contains("impl Render for VideoSvgView"));
    }

    #[test]
    fn format_names_round_trip() {
        for name in ["svg", "md", "html", "jsx", "gpui"] {
            assert!(Format::from_name(name).is_some());
        }
        assert!(Format::from_name("mp3").is_none());
    }
}
