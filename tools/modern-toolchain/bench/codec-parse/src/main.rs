//! Usage: git ls-files | modern-toolchain-bench <repo-root> <artifact> [compress|archive|parse|all]
//! Every figure is the best of BENCH_RUNS (default 7) in-process runs measured in thread CPU time (nodes are shared and loaded, wall time is not reproducible); sizes are exact byte counts.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use cpu_time::ThreadTime;

fn runs() -> usize {
    std::env::var("BENCH_RUNS").ok().and_then(|value| value.parse().ok()).filter(|runs| *runs > 0).unwrap_or(7)
}

fn best<T>(mut work: impl FnMut() -> T) -> (f64, T) {
    let mut fastest = f64::MAX;
    let mut last = None;
    for _ in 0..runs() {
        let start = ThreadTime::now();
        let value = work();
        fastest = fastest.min(start.elapsed().as_secs_f64() * 1000.0);
        last = Some(value);
    }
    (fastest, last.expect("runs() is nonzero"))
}

fn compress_report(name: &str, input: &[u8], compress: impl Fn(&[u8]) -> Vec<u8>, decompress: impl Fn(&[u8]) -> Vec<u8>) {
    let (compress_ms, packed) = best(|| compress(input));
    let (decompress_ms, unpacked) = best(|| decompress(&packed));
    let verdict = if unpacked == input { "roundtrip-ok" } else { "ROUNDTRIP-MISMATCH" };
    println!("{name:<28} {:>10} B  ratio {:.4}  compress {compress_ms:>9.1} ms  decompress {decompress_ms:>8.1} ms  {verdict}", packed.len(), packed.len() as f64 / input.len() as f64);
}

fn compress(input: &[u8]) {
    println!("-- compress {} B", input.len());
    compress_report(
        "flate2/zlib-rs 9",
        input,
        |data| {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
            encoder.write_all(data).expect("gzip write");
            encoder.finish().expect("gzip finish")
        },
        |data| {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(data).read_to_end(&mut out).expect("gzip read");
            out
        },
    );
    compress_report(
        "miniz_oxide 9 (zlib)",
        input,
        |data| miniz_oxide::deflate::compress_to_vec_zlib(data, 9),
        |data| miniz_oxide::inflate::decompress_to_vec_zlib(data).expect("miniz inflate"),
    );
    compress_report(
        "libdeflater 12 (gzip)",
        input,
        |data| {
            let mut compressor = libdeflater::Compressor::new(libdeflater::CompressionLvl::new(12).expect("level"));
            let mut out = vec![0; compressor.gzip_compress_bound(data.len())];
            let written = compressor.gzip_compress(data, &mut out).expect("deflate");
            out.truncate(written);
            out
        },
        |data| {
            let mut decompressor = libdeflater::Decompressor::new();
            let mut out = vec![0; input.len()];
            decompressor.gzip_decompress(data, &mut out).expect("inflate");
            out
        },
    );
    for level in [3, 19] {
        compress_report(
            &format!("zstd crate {level}"),
            input,
            |data| zstd::bulk::compress(data, level).expect("zstd"),
            |data| zstd::bulk::decompress(data, input.len() + 1024).expect("unzstd"),
        );
    }
    // ruzstd 0.9.1: the encoder panics with "not implemented" on some inputs, so it is probed apart and judged on decoding a zstd -19 stream.
    let probe = std::panic::catch_unwind(|| ruzstd::encoding::compress_to_vec(input, ruzstd::encoding::CompressionLevel::Fastest));
    match probe {
        Ok(packed) => println!("{:<28} {:>10} B  ratio {:.4}  (encoder ran)", "ruzstd fastest encoder", packed.len(), packed.len() as f64 / input.len() as f64),
        Err(_) => println!("{:<28} encoder panicked (not implemented in ruzstd 0.9.1) on this input", "ruzstd fastest encoder"),
    }
    let reference = zstd::bulk::compress(input, 19).expect("zstd");
    let (decompress_ms, unpacked) = best(|| {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(&reference[..]).expect("ruzstd header");
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).expect("ruzstd");
        out
    });
    println!("{:<28} decompress {decompress_ms:>8.1} ms of a zstd -19 stream  {}", "ruzstd decoder (pure Rust)", if unpacked == input { "roundtrip-ok" } else { "ROUNDTRIP-MISMATCH" });
    for quality in [5, 11] {
        compress_report(
            &format!("brotli crate q{quality}"),
            input,
            |data| {
                let mut out = Vec::new();
                let params = brotli::enc::BrotliEncoderParams { quality, ..Default::default() };
                brotli::BrotliCompress(&mut &data[..], &mut out, &params).expect("brotli");
                out
            },
            |data| {
                let mut out = Vec::new();
                brotli::BrotliDecompress(&mut &data[..], &mut out).expect("unbrotli");
                out
            },
        );
    }
    compress_report(
        "liblzma xz 9",
        input,
        |data| {
            let mut out = Vec::new();
            let mut encoder = liblzma::write::XzEncoder::new(&mut out, 9);
            encoder.write_all(data).expect("xz");
            encoder.finish().expect("xz finish");
            out
        },
        |data| {
            let mut out = Vec::new();
            liblzma::read::XzDecoder::new(data).read_to_end(&mut out).expect("unxz");
            out
        },
    );
    compress_report(
        "lzma-rs (pure Rust xz)",
        input,
        |data| {
            let mut out = Vec::new();
            lzma_rs::xz_compress(&mut &data[..], &mut out).expect("lzma-rs");
            out
        },
        |data| {
            let mut out = Vec::new();
            lzma_rs::xz_decompress(&mut &data[..], &mut out).expect("lzma-rs unxz");
            out
        },
    );
}

fn archive(root: &Path, files: &[PathBuf]) {
    let mut tar_bytes = Vec::new();
    let mut builder = tar::Builder::new(&mut tar_bytes);
    let mut packed_files = 0usize;
    let mut contents: Vec<(&PathBuf, Vec<u8>)> = Vec::new();
    let prefix = std::env::var("BENCH_ARCHIVE_PREFIX").unwrap_or_else(|_| "crates/".to_string());
    for path in files.iter().filter(|path| path.starts_with(&prefix)) {
        let Ok(data) = std::fs::read(root.join(path)) else { continue };
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        if builder.append_data(&mut header, path, &data[..]).is_ok() {
            packed_files += 1;
            contents.push((path, data));
        }
    }
    builder.finish().expect("tar finish");
    drop(builder);
    println!("-- archive of {prefix}: {packed_files} files, tar {} B", tar_bytes.len());
    compress_report(
        "tar crate + zstd crate 19",
        &tar_bytes,
        |data| zstd::bulk::compress(data, 19).expect("zstd"),
        |data| zstd::bulk::decompress(data, tar_bytes.len() + 1024).expect("unzstd"),
    );
    let total: usize = contents.iter().map(|(_, data)| data.len()).sum();
    for (label, method, level) in [("zip crate deflate 9", zip::CompressionMethod::Deflated, 9), ("zip crate zstd 19", zip::CompressionMethod::Zstd, 19)] {
        let (compress_ms, size) = best(|| {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default().compression_method(method).compression_level(Some(level));
            for (path, data) in &contents {
                writer.start_file(path.to_string_lossy(), options).expect("zip entry");
                writer.write_all(data).expect("zip write");
            }
            writer.finish().expect("zip finish").into_inner()
        });
        println!("{label:<28} {:>10} B  ratio {:.4}  compress {compress_ms:>9.1} ms  (zip of {packed_files} files, {total} B of content)", size.len(), size.len() as f64 / total as f64);
    }
}

fn read_all(root: &Path, files: &[PathBuf], suffixes: &[&str]) -> Vec<(PathBuf, Vec<u8>)> {
    files
        .iter()
        .filter(|path| suffixes.iter().any(|suffix| path.to_string_lossy().ends_with(suffix)))
        .filter_map(|path| std::fs::read(root.join(path)).ok().map(|bytes| (path.clone(), bytes)))
        .collect()
}

fn report(name: &str, ms: f64, bytes: usize, parsed: usize, total: usize) {
    println!("{name:<34} {ms:>9.1} ms  {:>8.1} MB/s  parsed {parsed}/{total}", bytes as f64 / 1e6 / (ms / 1000.0));
}

fn parse(root: &Path, files: &[PathBuf], artifact_json: Option<&Path>) {
    if let Some(path) = artifact_json {
        let text = std::fs::read_to_string(path).expect("json file");
        let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
        println!("-- JSONL {} lines {} B ({})", lines.len(), text.len(), path.display());
        let (ms, count) = best(|| lines.iter().filter(|line| serde_json::from_str::<serde_json::Value>(line).is_ok()).count());
        report("serde_json::Value", ms, text.len(), count, lines.len());
        let (ms, count) = best(|| lines.iter().filter(|line| sonic_rs::from_str::<sonic_rs::Value>(line).is_ok()).count());
        report("sonic_rs::Value", ms, text.len(), count, lines.len());
        let (ms, count) = best(|| {
            lines
                .iter()
                .filter(|line| {
                    let mut owned = line.as_bytes().to_vec();
                    simd_json::to_owned_value(&mut owned).is_ok()
                })
                .count()
        });
        report("simd_json owned (incl. copy)", ms, text.len(), count, lines.len());
    }

    let yaml = read_all(root, files, &[".yml", ".yaml"]);
    let yaml_bytes: usize = yaml.iter().map(|(_, bytes)| bytes.len()).sum();
    println!("-- YAML {} files {} B", yaml.len(), yaml_bytes);
    let texts: Vec<&str> = yaml.iter().filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok()).collect();
    let (ms, count) = best(|| texts.iter().filter(|text| serde_norway::from_str::<serde_json::Value>(text).is_ok()).count());
    report("serde_norway -> Value", ms, yaml_bytes, count, texts.len());
    let (ms, count) = best(|| texts.iter().filter(|text| serde_saphyr::from_str::<serde_json::Value>(text).is_ok()).count());
    report("serde-saphyr -> Value", ms, yaml_bytes, count, texts.len());
    let (ms, count) = best(|| texts.iter().filter(|text| yaml_rust2::YamlLoader::load_from_str(text).is_ok()).count());
    report("yaml-rust2 YamlLoader", ms, yaml_bytes, count, texts.len());
    let (ms, count) = best(|| {
        texts
            .iter()
            .filter(|text| {
                use saphyr::LoadableYamlNode;
                saphyr::Yaml::load_from_str(text).is_ok()
            })
            .count()
    });
    report("saphyr Yaml", ms, yaml_bytes, count, texts.len());

    let tables = read_all(root, files, &[".tsv", ".csv"]);
    let table_bytes: usize = tables.iter().map(|(_, bytes)| bytes.len()).sum();
    println!("-- CSV/TSV {} files {} B", tables.len(), table_bytes);
    let delimiter_of = |path: &Path| if path.extension().is_some_and(|extension| extension == "tsv") { b'\t' } else { b',' };
    let (ms, rows) = best(|| {
        tables
            .iter()
            .map(|(path, bytes)| csv::ReaderBuilder::new().delimiter(delimiter_of(path)).has_headers(false).flexible(true).from_reader(&bytes[..]).byte_records().filter(|record| record.is_ok()).count())
            .sum::<usize>()
    });
    report("csv ByteRecord", ms, table_bytes, rows, rows);
    let (ms, rows) = best(|| {
        tables
            .iter()
            .map(|(path, bytes)| csv::ReaderBuilder::new().delimiter(delimiter_of(path)).has_headers(false).flexible(true).from_reader(&bytes[..]).records().filter(|record| record.is_ok()).count())
            .sum::<usize>()
    });
    report("csv StringRecord", ms, table_bytes, rows, rows);
    let (ms, rows) = best(|| {
        tables
            .iter()
            .map(|(path, bytes)| {
                let delimiter = delimiter_of(path) as char;
                std::str::from_utf8(bytes).map_or(0, |text| text.lines().map(|line| line.split(delimiter).count()).count())
            })
            .sum::<usize>()
    });
    report("str::split (no quoting)", ms, table_bytes, rows, rows);

    let manifests = read_all(root, files, &["Cargo.toml", "Cargo.lock", "steps.toml", "build.toml"]);
    let manifest_bytes: usize = manifests.iter().map(|(_, bytes)| bytes.len()).sum();
    println!("-- TOML {} files {} B", manifests.len(), manifest_bytes);
    let texts: Vec<&str> = manifests.iter().filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok()).collect();
    let (ms, count) = best(|| texts.iter().filter(|text| text.parse::<toml::Table>().is_ok()).count());
    report("toml::Table", ms, manifest_bytes, count, texts.len());

    let sources = read_all(root, files, &[".rs"]);
    let source_bytes: usize = sources.iter().map(|(_, bytes)| bytes.len()).sum();
    println!("-- Rust source {} files {} B", sources.len(), source_bytes);
    let texts: Vec<&str> = sources.iter().filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok()).collect();
    let (ms, count) = best(|| texts.iter().filter(|text| syn::parse_file(text).is_ok()).count());
    report("syn::parse_file", ms, source_bytes, count, texts.len());
    let (ms, count) = best(|| {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).expect("rust grammar");
        texts.iter().filter(|text| parser.parse(text, None).is_some_and(|tree| !tree.root_node().has_error())).count()
    });
    report("tree-sitter-rust (no errors)", ms, source_bytes, count, texts.len());

    let documents = read_all(root, files, &[".md"]);
    let document_bytes: usize = documents.iter().map(|(_, bytes)| bytes.len()).sum();
    println!("-- Markdown {} files {} B", documents.len(), document_bytes);
    let texts: Vec<&str> = documents.iter().filter_map(|(_, bytes)| std::str::from_utf8(bytes).ok()).collect();
    let (ms, count) = best(|| {
        texts
            .iter()
            .map(|text| {
                let mut html = String::new();
                pulldown_cmark::html::push_html(&mut html, pulldown_cmark::Parser::new(text));
                html.len()
            })
            .sum::<usize>()
    });
    report("pulldown-cmark html", ms, document_bytes, count, count);
    let (ms, count) = best(|| texts.iter().map(|text| comrak::markdown_to_html(text, &comrak::Options::default()).len()).sum::<usize>());
    report("comrak html", ms, document_bytes, count, count);
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(arguments.get(1).expect("repo root"));
    let artifact = arguments.get(2).map(PathBuf::from);
    let mode = arguments.get(3).map_or("all", String::as_str);
    let mut listing = String::new();
    std::io::stdin().read_to_string(&mut listing).expect("file list on stdin");
    let files: Vec<PathBuf> = listing.lines().map(PathBuf::from).collect();
    if mode == "compress" || mode == "all" {
        let artifact = artifact.as_ref().expect("artifact path");
        compress(&std::fs::read(artifact).expect("artifact"));
    }
    if mode == "archive" || mode == "all" {
        archive(&root, &files);
    }
    if mode == "parse" || mode == "all" {
        parse(&root, &files, std::env::var_os("BENCH_JSONL").map(PathBuf::from).as_deref());
    }
}
