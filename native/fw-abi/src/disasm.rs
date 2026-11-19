use craft_pe::headers;
use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisasmInstruction {
    pub rip: String,
    pub bytes: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisasmReport {
    pub ok: bool,
    pub path: String,
    pub bitness: u32,
    pub entry_rva: String,
    pub entry_va: String,
    pub file_offset: usize,
    pub count: usize,
    pub instructions: Vec<DisasmInstruction>,
}

pub fn disassemble_pe_entry(path: &Path, max_insns: usize) -> Result<DisasmReport, String> {
    let mut file = File::open(path).map_err(|e| format!("open failed: {e}"))?;
    let mut header_buf = vec![0u8; 64 * 1024];
    let n = file.read(&mut header_buf).map_err(|e| format!("read header failed: {e}"))?;
    header_buf.truncate(n);

    let pe = headers(&header_buf).map_err(|e| format!("pe parse failed: {e}"))?;
    let bitness = match pe.machine {
        0x8664 => 64,
        0x014c => 32,
        other => return Err(format!("unsupported x86 machine 0x{other:04x}")),
    };
    let entry_off = pe
        .rva_to_offset(pe.entry_rva)
        .ok_or_else(|| format!("entry RVA 0x{:x} does not map to file offset", pe.entry_rva))?;
    let entry_va = pe.image_va(u64::from(pe.entry_rva)).unwrap_or(u64::from(pe.entry_rva));

    let cap = max_insns.clamp(1, 256);
    let read_len = cap * 15;
    file.seek(SeekFrom::Start(entry_off as u64))
        .map_err(|e| format!("seek failed: {e}"))?;
    let mut code = vec![0u8; read_len];
    let got = file.read(&mut code).map_err(|e| format!("read code failed: {e}"))?;
    code.truncate(got);

    let mut decoder = Decoder::with_ip(bitness, &code, entry_va, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut instructions = Vec::with_capacity(cap);

    while decoder.can_decode() && instructions.len() < cap {
        let insn = decoder.decode();
        let mut text = String::new();
        formatter.format(&insn, &mut text);
        let start = (insn.ip() - entry_va) as usize;
        let end = (start + insn.len()).min(code.len());
        let mut bytes_hex = String::with_capacity((end - start) * 2);
        for &b in &code[start..end] {
            bytes_hex.push_str(&format!("{b:02x}"));
        }
        instructions.push(DisasmInstruction {
            rip: format!("0x{:x}", insn.ip()),
            bytes: bytes_hex,
            text,
        });
    }

    Ok(DisasmReport {
        ok: true,
        path: path.display().to_string(),
        bitness,
        entry_rva: format!("0x{:x}", pe.entry_rva),
        entry_va: format!("0x{entry_va:x}"),
        file_offset: entry_off,
        count: instructions.len(),
        instructions,
    })
}
