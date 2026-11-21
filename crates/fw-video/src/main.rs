//! `fw-video <input.mp4> <out-dir> [--to svg,md,html,jsx,gpui] [--fps 12] [--width 640] [--colors 6]
//!           [--speckle 2] [--scene 0.12] [--max-frames N]`
//!
//! Decodes and vectorises the video in pure Rust and writes the requested formats into out-dir,
//! then prints a JSON summary on stdout.

use fw_video::export::{write_all, Format, Source};
use fw_video::{convert_file, Options};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: fw-video <input.mp4> <out-dir> [--to svg,md,html,jsx,gpui] [--fps 12] [--width 640] [--colors 6] [--speckle 2] [--scene 0.12] [--max-frames N]";

fn run(args: &[String]) -> Result<String, String> {
    let [input, out_dir, ..] = args else {
        return Err(USAGE.into());
    };
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    let num = |name: &str| -> Result<Option<f64>, String> {
        flag(name).map(|v| v.parse::<f64>().map_err(|_| format!("{name} expects a number, got `{v}`"))).transpose()
    };
    let formats: Vec<Format> = match flag("--to") {
        None => Format::ALL.to_vec(),
        Some(list) => list
            .split(',')
            .map(|n| Format::from_name(n.trim()).ok_or(format!("--to: unknown format `{n}` (svg, md, html, jsx, gpui)")))
            .collect::<Result<_, _>>()?,
    };
    let mut opts = Options::default();
    if let Some(v) = num("--fps")? {
        opts.fps = v;
    }
    if let Some(v) = num("--width")? {
        opts.width = (v > 0.0).then_some(v as u32);
    }
    if let Some(v) = num("--colors")? {
        opts.color_precision = v as i32;
    }
    if let Some(v) = num("--speckle")? {
        opts.filter_speckle = v as usize;
    }
    if let Some(v) = num("--scene")? {
        opts.scene_threshold = v;
    }
    opts.max_frames = num("--max-frames")?.map(|v| v as usize);

    let mut frames = Vec::new();
    let decoded = convert_file(Path::new(input), &opts, |f| frames.push(f)).map_err(|e| format!("{input}: {e}"))?;
    if frames.is_empty() {
        return Err(format!("{input}: no frame decoded"));
    }
    let name = Path::new(input).file_name().map_or_else(|| input.clone(), |n| n.to_string_lossy().into_owned());
    let out = PathBuf::from(out_dir);
    let written = write_all(&out, &frames, &Source { name, decoded_frames: decoded }, &formats).map_err(|e| format!("{}: {e}", out.display()))?;
    let scenes = frames.iter().filter(|f| f.scene_start).count();
    let files: Vec<String> = written.iter().filter(|p| !p.to_string_lossy().contains("frame-")).map(|p| format!("{:?}", p.to_string_lossy())).collect();
    Ok(format!(
        "{{\"input\":{input:?},\"decoded\":{decoded},\"frames\":{},\"scenes\":{scenes},\"width\":{},\"height\":{},\"paths\":{},\"files\":[{}]}}",
        frames.len(),
        frames[0].width,
        frames[0].height,
        frames.iter().map(|f| f.path_count).sum::<usize>(),
        files.join(",")
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("fw-video: {e}");
            ExitCode::from(2)
        }
    }
}
