use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GgufInspectionReport {
    pub ok: bool,
    pub path: String,
    pub version: u32,
    pub tensor_count: u64,
    pub kv_count: u64,
    pub metadata: BTreeMap<String, String>,
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.read_bytes(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.read_bytes(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.read_bytes(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.read_bytes(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.read_bytes(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.read_bytes(8)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.read_bytes(8)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.read_bytes(8)?.try_into().ok()?))
    }
    fn gguf_string(&mut self) -> Option<String> {
        let len = usize::try_from(self.u64()?).ok()?;
        if len > 16 * 1024 * 1024 {
            return None;
        }
        let b = self.read_bytes(len)?;
        Some(String::from_utf8_lossy(b).into_owned())
    }
    fn read_value(&mut self, vtype: u32) -> Option<String> {
        match vtype {
            0 => Some(self.u8()?.to_string()),
            1 => Some((self.u8()? as i8).to_string()),
            2 => Some(self.u16()?.to_string()),
            3 => Some((self.u16()? as i16).to_string()),
            4 => Some(self.u32()?.to_string()),
            5 => Some(self.i32()?.to_string()),
            6 => Some(self.f32()?.to_string()),
            7 => Some((self.u8()? != 0).to_string()),
            8 => self.gguf_string(),
            9 => {
                let elem_type = self.u32()?;
                let count = self.u64()?;
                let mut preview = Vec::new();
                for i in 0..count {
                    let val = self.read_value(elem_type)?;
                    if i < 4 {
                        preview.push(val);
                    }
                }
                Some(format!("[len={count} preview={preview:?}]"))
            }
            10 => Some(self.u64()?.to_string()),
            11 => Some(self.i64()?.to_string()),
            12 => Some(self.f64()?.to_string()),
            _ => None,
        }
    }
}

pub fn inspect_gguf_bytes(buf: &[u8], path_label: &str) -> Result<GgufInspectionReport, String> {
    let mut c = Cursor { buf, pos: 0 };
    let magic = c.read_bytes(4).ok_or("truncated GGUF magic")?;
    if magic != b"GGUF" {
        return Err("invalid GGUF magic (expected 'GGUF')".to_string());
    }
    let version = c.u32().ok_or("truncated GGUF version")?;
    let tensor_count = c.u64().ok_or("truncated tensor count")?;
    let kv_count = c.u64().ok_or("truncated kv count")?;
    let mut metadata = BTreeMap::new();
    let max_kv = kv_count.min(128);
    for _ in 0..max_kv {
        let Some(key) = c.gguf_string() else { break };
        let Some(vtype) = c.u32() else { break };
        let Some(val) = c.read_value(vtype) else { break };
        metadata.insert(key, val);
    }
    Ok(GgufInspectionReport {
        ok: true,
        path: path_label.to_string(),
        version,
        tensor_count,
        kv_count,
        metadata,
    })
}

pub fn inspect_gguf_path(path: &Path) -> Result<GgufInspectionReport, String> {
    let mut f = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let mut buf = vec![0u8; 256 * 1024];
    let n = f.read(&mut buf).map_err(|e| format!("read failed: {e}"))?;
    buf.truncate(n);
    inspect_gguf_bytes(&buf, &path.display().to_string())
}

/// Fat-LTO pure-Rust GGML vector kernels (dot product, RMSNorm, Softmax, cosine similarity).
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn rmsnorm_f32(x: &[f32], weight: &[f32], eps: f32) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let mean_sq = x.iter().map(|v| v * v).sum::<f32>() / (x.len() as f32);
    let inv = 1.0 / (mean_sq + eps).sqrt();
    x.iter()
        .zip(weight.iter())
        .map(|(v, w)| v * inv * w)
        .collect()
}

pub fn softmax_f32(logits: &[f32]) -> Vec<f32> {
    if logits.is_empty() {
        return Vec::new();
    }
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|&v| (v - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum == 0.0 {
        return vec![0.0; logits.len()];
    }
    exps.into_iter().map(|e| e / sum).collect()
}

pub fn cosine_similarity_f32(a: &[f32], b: &[f32]) -> f32 {
    let denom = dot_f32(a, a).sqrt() * dot_f32(b, b).sqrt();
    if denom == 0.0 { 0.0 } else { dot_f32(a, b) / denom }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetensorsHeaderReport {
    pub ok: bool,
    pub header_bytes: u64,
    pub tensor_count: usize,
    pub tensor_names: Vec<String>,
}

/// Zero-copy Safetensors header parser (8-byte LE length + UTF-8 JSON header).
pub fn inspect_safetensors_bytes(buf: &[u8]) -> Result<SafetensorsHeaderReport, String> {
    if buf.len() < 8 {
        return Err("truncated safetensors header length".to_string());
    }
    let header_len = u64::from_le_bytes(buf[..8].try_into().map_err(|_| "header len slice")?);
    let len_usize = usize::try_from(header_len).map_err(|_| "header len overflow")?;
    if len_usize == 0 || len_usize > 64 * 1024 * 1024 || buf.len() < 8 + len_usize {
        return Err("invalid or truncated safetensors JSON header".to_string());
    }
    let raw = std::str::from_utf8(&buf[8..8 + len_usize]).map_err(|e| format!("utf8 error: {e}"))?;
    let val: serde_json::Value = serde_json::from_str(raw).map_err(|e| format!("json error: {e}"))?;
    let obj = val.as_object().ok_or("safetensors header is not a JSON object")?;
    let tensor_names: Vec<String> = obj
        .keys()
        .filter(|k| k.as_str() != "__metadata__")
        .take(64)
        .cloned()
        .collect();
    let tensor_count = obj.keys().filter(|k| k.as_str() != "__metadata__").count();
    Ok(SafetensorsHeaderReport {
        ok: true,
        header_bytes: header_len,
        tensor_count,
        tensor_names,
    })
}
