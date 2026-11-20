//! GGUF v2/v3 reader and writer: header, metadata KV, tensor infos and sidecar naming.

use crate::GgmlType;
use serde::{Deserialize, Serialize};

/// Official GGUF magic bytes (`"GGUF"` in little-endian: 0x46554747).
pub const GGUF_MAGIC: [u8; 4] = *b"GGUF";
pub const GGUF_DEFAULT_ALIGNMENT: u64 = 32;

/// GGUF header descriptor (compatible with `llama.cpp` / `ggml-org/convert`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GgufHeader {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_kv_count: u64,
}

pub fn parse_gguf_header(bytes: &[u8]) -> Result<GgufHeader, String> {
    if bytes.len() < 24 {
        return Err("GGUF header too short (<24 bytes)".into());
    }
    if bytes[0..4] != GGUF_MAGIC {
        return Err("Invalid GGUF magic".into());
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if !(2..=3).contains(&version) {
        return Err(format!("Unsupported GGUF version {version}"));
    }
    let tensor_count = u64::from_le_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]);
    let metadata_kv_count = u64::from_le_bytes([
        bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
    ]);
    Ok(GgufHeader {
        version,
        tensor_count,
        metadata_kv_count,
    })
}

pub fn encode_gguf_header(version: u32, tensor_count: u64, metadata_kv_count: u64) -> [u8; 24] {
    let mut buf = [0u8; 24];
    buf[0..4].copy_from_slice(&GGUF_MAGIC);
    buf[4..8].copy_from_slice(&version.to_le_bytes());
    buf[8..16].copy_from_slice(&tensor_count.to_le_bytes());
    buf[16..24].copy_from_slice(&metadata_kv_count.to_le_bytes());
    buf
}

/// Metadata value (`enum gguf_type`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GgufValue {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
    U32(u32),
    I32(i32),
    F32(f32),
    Bool(bool),
    Str(String),
    Array(Vec<GgufValue>),
    U64(u64),
    I64(i64),
    F64(f64),
}

impl GgufValue {
    fn type_id(&self) -> u32 {
        match self {
            Self::U8(_) => 0,
            Self::I8(_) => 1,
            Self::U16(_) => 2,
            Self::I16(_) => 3,
            Self::U32(_) => 4,
            Self::I32(_) => 5,
            Self::F32(_) => 6,
            Self::Bool(_) => 7,
            Self::Str(_) => 8,
            Self::Array(_) => 9,
            Self::U64(_) => 10,
            Self::I64(_) => 11,
            Self::F64(_) => 12,
        }
    }
}

/// Tensor descriptor from the GGUF tensor-info table; `offset` is relative to the data section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufTensorInfo {
    pub name: String,
    pub ne: Vec<u64>,
    pub dtype: GgmlType,
    pub offset: u64,
}

impl GgufTensorInfo {
    pub fn nbytes(&self) -> Result<u64, String> {
        let ne0 = *self.ne.first().ok_or("tensor without dimensions")?;
        let row = self
            .dtype
            .row_size(usize::try_from(ne0).map_err(|e| e.to_string())?)
            .ok_or_else(|| format!("{}: ne0={ne0} is not a multiple of block size {}", self.name, self.dtype.block_size()))?;
        self.ne[1..]
            .iter()
            .try_fold(row as u64, |acc, &n| acc.checked_mul(n))
            .ok_or_else(|| format!("{}: tensor size overflow", self.name))
    }
}

/// Parsed GGUF file layout (the tensor data itself stays in the caller's buffer).
#[derive(Debug, Clone)]
pub struct GgufFile {
    pub header: GgufHeader,
    pub metadata: Vec<(String, GgufValue)>,
    pub tensors: Vec<GgufTensorInfo>,
    pub alignment: u64,
    /// Absolute byte offset of the tensor data section.
    pub data_offset: usize,
}

impl GgufFile {
    pub fn tensor_data<'a>(&self, bytes: &'a [u8], t: &GgufTensorInfo) -> Result<&'a [u8], String> {
        let start = self.data_offset as u64 + t.offset;
        let end = start + t.nbytes()?;
        bytes
            .get(start as usize..end as usize)
            .ok_or_else(|| format!("{}: data out of bounds", t.name))
    }
}

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.b.len()).ok_or("GGUF truncated")?;
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], String> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn string(&mut self) -> Result<String, String> {
        let len = usize::try_from(self.u64()?).map_err(|e| e.to_string())?;
        String::from_utf8(self.take(len)?.to_vec()).map_err(|e| e.to_string())
    }
    fn value(&mut self, ty: u32) -> Result<GgufValue, String> {
        Ok(match ty {
            0 => GgufValue::U8(self.arr::<1>()?[0]),
            1 => GgufValue::I8(self.arr::<1>()?[0] as i8),
            2 => GgufValue::U16(u16::from_le_bytes(self.arr()?)),
            3 => GgufValue::I16(i16::from_le_bytes(self.arr()?)),
            4 => GgufValue::U32(self.u32()?),
            5 => GgufValue::I32(i32::from_le_bytes(self.arr()?)),
            6 => GgufValue::F32(f32::from_le_bytes(self.arr()?)),
            7 => GgufValue::Bool(self.arr::<1>()?[0] != 0),
            8 => GgufValue::Str(self.string()?),
            9 => {
                let et = self.u32()?;
                let n = usize::try_from(self.u64()?).map_err(|e| e.to_string())?;
                if n > self.b.len() - self.pos {
                    return Err("GGUF array longer than file".into());
                }
                GgufValue::Array((0..n).map(|_| self.value(et)).collect::<Result<_, _>>()?)
            }
            10 => GgufValue::U64(self.u64()?),
            11 => GgufValue::I64(i64::from_le_bytes(self.arr()?)),
            12 => GgufValue::F64(f64::from_le_bytes(self.arr()?)),
            _ => return Err(format!("unknown GGUF value type {ty}")),
        })
    }
}

fn alignment_of(metadata: &[(String, GgufValue)]) -> u64 {
    metadata
        .iter()
        .find_map(|(k, v)| match (k.as_str(), v) {
            ("general.alignment", GgufValue::U32(a)) => Some(u64::from(*a)),
            _ => None,
        })
        .unwrap_or(GGUF_DEFAULT_ALIGNMENT)
}

fn align_up(n: usize, a: u64) -> usize {
    let a = a.max(1) as usize;
    n.div_ceil(a) * a
}

/// Parse header, metadata and tensor infos; every `ggml_type` of `GgmlType::ALL` is accepted.
pub fn parse_gguf(bytes: &[u8]) -> Result<GgufFile, String> {
    let header = parse_gguf_header(bytes)?;
    let mut r = Reader { b: bytes, pos: 24 };
    let mut metadata = Vec::new();
    for _ in 0..header.metadata_kv_count {
        let key = r.string()?;
        let ty = r.u32()?;
        metadata.push((key, r.value(ty)?));
    }
    let mut tensors = Vec::new();
    for _ in 0..header.tensor_count {
        let name = r.string()?;
        let n_dims = r.u32()? as usize;
        if !(1..=4).contains(&n_dims) {
            return Err(format!("{name}: invalid n_dims {n_dims}"));
        }
        let ne = (0..n_dims).map(|_| r.u64()).collect::<Result<Vec<_>, _>>()?;
        let id = r.u32()?;
        let dtype = GgmlType::from_id(id).ok_or_else(|| format!("{name}: unsupported ggml type id {id}"))?;
        tensors.push(GgufTensorInfo { name, ne, dtype, offset: r.u64()? });
    }
    let alignment = alignment_of(&metadata);
    Ok(GgufFile { header, metadata, tensors, alignment, data_offset: align_up(r.pos, alignment) })
}

fn put_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_value(out: &mut Vec<u8>, v: &GgufValue) {
    match v {
        GgufValue::U8(x) => out.push(*x),
        GgufValue::I8(x) => out.push(*x as u8),
        GgufValue::U16(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::I16(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::U32(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::I32(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::F32(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::Bool(x) => out.push(u8::from(*x)),
        GgufValue::Str(s) => put_string(out, s),
        GgufValue::Array(items) => {
            out.extend_from_slice(&items.first().map_or(4, GgufValue::type_id).to_le_bytes());
            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
            items.iter().for_each(|i| put_value(out, i));
        }
        GgufValue::U64(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::I64(x) => out.extend_from_slice(&x.to_le_bytes()),
        GgufValue::F64(x) => out.extend_from_slice(&x.to_le_bytes()),
    }
}

/// Serialize a GGUF file; each tensor is `(name, ne, dtype, raw bytes)`, offsets are assigned
/// sequentially and padded to the alignment (`general.alignment` or 32).
pub fn encode_gguf(
    version: u32,
    metadata: &[(String, GgufValue)],
    tensors: &[(&str, &[u64], GgmlType, &[u8])],
) -> Result<Vec<u8>, String> {
    let alignment = alignment_of(metadata);
    let mut out = encode_gguf_header(version, tensors.len() as u64, metadata.len() as u64).to_vec();
    for (k, v) in metadata {
        put_string(&mut out, k);
        out.extend_from_slice(&v.type_id().to_le_bytes());
        put_value(&mut out, v);
    }
    let mut data = Vec::new();
    for (name, ne, dtype, bytes) in tensors {
        data.resize(align_up(data.len(), alignment), 0);
        let info = GgufTensorInfo { name: (*name).into(), ne: ne.to_vec(), dtype: *dtype, offset: data.len() as u64 };
        let expected = info.nbytes()?;
        if expected != bytes.len() as u64 {
            return Err(format!("{name}: expected {expected} bytes, got {}", bytes.len()));
        }
        put_string(&mut out, name);
        out.extend_from_slice(&(ne.len() as u32).to_le_bytes());
        ne.iter().for_each(|n| out.extend_from_slice(&n.to_le_bytes()));
        out.extend_from_slice(&dtype.id().to_le_bytes());
        out.extend_from_slice(&info.offset.to_le_bytes());
        data.extend_from_slice(bytes);
    }
    out.resize(align_up(out.len(), alignment), 0);
    out.extend_from_slice(&data);
    Ok(out)
}

/// Companion GGUF files resolved next to a model by `llama.cpp` (`common/download.cpp`
/// `common_download_sidecars`): matched by a keyword in the file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GgufSidecar {
    Mmproj,
    Mtp,
    Dflash,
    Eagle3,
    Dspark,
}

impl GgufSidecar {
    pub const ALL: [GgufSidecar; 5] = [Self::Mmproj, Self::Mtp, Self::Dflash, Self::Eagle3, Self::Dspark];

    pub fn name(self) -> &'static str {
        match self {
            Self::Mmproj => "mmproj",
            Self::Mtp => "mtp",
            Self::Dflash => "dflash",
            Self::Eagle3 => "eagle3",
            Self::Dspark => "dspark",
        }
    }

    /// Path keyword upstream searches for (`mmproj`, then `mtp-`, `dflash-`, ...).
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Mmproj => "mmproj",
            Self::Mtp => "mtp-",
            Self::Dflash => "dflash-",
            Self::Eagle3 => "eagle3-",
            Self::Dspark => "dspark-",
        }
    }

    /// Classify a `.gguf` path; `None` for a plain model file.
    pub fn of_path(path: &str) -> Option<Self> {
        path.ends_with(".gguf")
            .then(|| Self::ALL.into_iter().find(|s| path.contains(s.keyword())))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_new_types_and_metadata() {
        let nvfp4 = vec![0xA5u8; GgmlType::NVFP4.row_size(128).unwrap() * 2];
        let mxfp4 = vec![0x5Au8; GgmlType::MXFP4.row_size(64).unwrap()];
        let f8 = vec![0x38u8; 7];
        let meta = vec![
            ("general.architecture".to_string(), GgufValue::Str("llama".into())),
            ("llama.block_count".to_string(), GgufValue::U32(2)),
            ("tokenizer.ggml.scores".to_string(), GgufValue::Array(vec![GgufValue::F32(0.5), GgufValue::F32(-1.0)])),
            ("general.file_type".to_string(), GgufValue::I64(-7)),
        ];
        let bytes = encode_gguf(
            3,
            &meta,
            &[
                ("a.weight", &[128, 2], GgmlType::NVFP4, &nvfp4),
                ("b.weight", &[64], GgmlType::MXFP4, &mxfp4),
                ("c.scale", &[7], GgmlType::F8_E4M3, &f8),
            ],
        )
        .unwrap();
        let f = parse_gguf(&bytes).unwrap();
        assert_eq!(f.header.version, 3);
        assert_eq!(f.metadata, meta);
        assert_eq!(f.data_offset % 32, 0);
        let dtypes: Vec<_> = f.tensors.iter().map(|t| t.dtype).collect();
        assert_eq!(dtypes, [GgmlType::NVFP4, GgmlType::MXFP4, GgmlType::F8_E4M3]);
        assert_eq!(f.tensors[0].ne, [128, 2]);
        assert_eq!(f.tensors[1].offset % 32, 0);
        assert_eq!(f.tensor_data(&bytes, &f.tensors[0]).unwrap(), &nvfp4[..]);
        assert_eq!(f.tensor_data(&bytes, &f.tensors[1]).unwrap(), &mxfp4[..]);
        assert_eq!(f.tensor_data(&bytes, &f.tensors[2]).unwrap(), &f8[..]);
    }

    #[test]
    fn every_type_roundtrips() {
        for t in GgmlType::ALL {
            let n = t.block_size() * 2;
            let data = vec![1u8; t.row_size(n).unwrap()];
            let bytes = encode_gguf(3, &[], &[("w", &[n as u64], t, &data)]).unwrap();
            let f = parse_gguf(&bytes).unwrap();
            assert_eq!(f.tensors[0].dtype, t);
            assert_eq!(f.tensor_data(&bytes, &f.tensors[0]).unwrap(), &data[..]);
        }
    }

    #[test]
    fn rejects_bad_input() {
        let mut bytes = encode_gguf(3, &[], &[("w", &[32], GgmlType::Q4_0, &[0u8; 18])]).unwrap();
        assert!(encode_gguf(3, &[], &[("w", &[31], GgmlType::Q4_0, &[0u8; 18])]).is_err());
        assert!(parse_gguf(&bytes[..30]).is_err());
        let type_pos = bytes.windows(4).position(|w| w == 2u32.to_le_bytes()).unwrap();
        bytes[type_pos..type_pos + 4].copy_from_slice(&31u32.to_le_bytes());
        assert!(parse_gguf(&bytes).unwrap_err().contains("unsupported ggml type id 31"));
    }

    #[test]
    fn sidecar_names_follow_llama_cpp() {
        assert_eq!(GgufSidecar::of_path("org/m/mmproj-F16.gguf"), Some(GgufSidecar::Mmproj));
        assert_eq!(GgufSidecar::of_path("mtp-Q4_K_M.gguf"), Some(GgufSidecar::Mtp));
        assert_eq!(GgufSidecar::of_path("dflash-Q8_0.gguf"), Some(GgufSidecar::Dflash));
        assert_eq!(GgufSidecar::of_path("model-Q4_K_M.gguf"), None);
        assert_eq!(GgufSidecar::of_path("mmproj-F16.bin"), None);
    }
}
