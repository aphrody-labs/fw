//! Pure-Rust `#![forbid(unsafe_code)]` HLS (M3U8) playlist parser.
#![forbid(unsafe_code)]

/// A single media segment in an HLS playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsSegment {
    /// Segment duration in seconds.
    pub duration_secs: f64,
    /// Segment URI.
    pub uri: String,
}

/// Parses an HLS media playlist (`#EXTM3U` / `#EXTINF`) into segments.
#[must_use]
pub fn parse_media_playlist(input: &str) -> Vec<HlsSegment> {
    let mut segments = Vec::new();
    let mut pending_duration: Option<f64> = None;

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let dur_str = rest.split(',').next().unwrap_or("").trim();
            pending_duration = dur_str.parse::<f64>().ok();
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some(duration_secs) = pending_duration.take() {
            segments.push(HlsSegment {
                duration_secs,
                uri: line.to_string(),
            });
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_hls_playlist() {
        let m3u8 = "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:4.000,\nseg0.ts\n#EXTINF:3.500,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let segs = parse_media_playlist(m3u8);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].uri, "seg0.ts");
        assert!((segs[0].duration_secs - 4.0).abs() < f64::EPSILON);
        assert_eq!(segs[1].uri, "seg1.ts");
    }
}
