//! `enum ggml_type` ids, block sizes and byte sizes, taken from `ggml-org/ggml` master.

use serde::{Deserialize, Serialize};

/// GGML tensor data types matching `enum ggml_type` in `ggml-org/ggml` `include/ggml.h`
/// (`GGML_TYPE_COUNT = 44`; ids 4, 5, 31..33 and 36..38 are removed upstream).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
#[allow(non_camel_case_types)]
pub enum GgmlType {
    F32 = 0,
    F16 = 1,
    Q4_0 = 2,
    Q4_1 = 3,
    Q5_0 = 6,
    Q5_1 = 7,
    Q8_0 = 8,
    Q8_1 = 9,
    Q2K = 10,
    Q3K = 11,
    Q4K = 12,
    Q5K = 13,
    Q6K = 14,
    Q8K = 15,
    IQ2_XXS = 16,
    IQ2_XS = 17,
    IQ3_XXS = 18,
    IQ1_S = 19,
    IQ4_NL = 20,
    IQ3_S = 21,
    IQ2_S = 22,
    IQ4_XS = 23,
    I8 = 24,
    I16 = 25,
    I32 = 26,
    I64 = 27,
    F64 = 28,
    IQ1_M = 29,
    BF16 = 30,
    TQ1_0 = 34,
    TQ2_0 = 35,
    MXFP4 = 39,
    NVFP4 = 40,
    Q1_0 = 41,
    Q2_0 = 42,
    F8_E4M3 = 43,
}

impl GgmlType {
    /// Every live `ggml_type`, in id order.
    pub const ALL: [GgmlType; 36] = [
        Self::F32, Self::F16, Self::Q4_0, Self::Q4_1, Self::Q5_0, Self::Q5_1, Self::Q8_0,
        Self::Q8_1, Self::Q2K, Self::Q3K, Self::Q4K, Self::Q5K, Self::Q6K, Self::Q8K,
        Self::IQ2_XXS, Self::IQ2_XS, Self::IQ3_XXS, Self::IQ1_S, Self::IQ4_NL, Self::IQ3_S,
        Self::IQ2_S, Self::IQ4_XS, Self::I8, Self::I16, Self::I32, Self::I64, Self::F64,
        Self::IQ1_M, Self::BF16, Self::TQ1_0, Self::TQ2_0, Self::MXFP4, Self::NVFP4,
        Self::Q1_0, Self::Q2_0, Self::F8_E4M3,
    ];

    /// Numeric id as stored in GGUF tensor infos.
    pub fn id(self) -> u32 {
        self as u32
    }

    /// Decode a GGUF tensor type id; `None` for removed or unknown ids.
    pub fn from_id(id: u32) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.id() == id)
    }

    /// `type_name` from the upstream `type_traits` table.
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F16 => "f16",
            Self::Q4_0 => "q4_0",
            Self::Q4_1 => "q4_1",
            Self::Q5_0 => "q5_0",
            Self::Q5_1 => "q5_1",
            Self::Q8_0 => "q8_0",
            Self::Q8_1 => "q8_1",
            Self::Q2K => "q2_K",
            Self::Q3K => "q3_K",
            Self::Q4K => "q4_K",
            Self::Q5K => "q5_K",
            Self::Q6K => "q6_K",
            Self::Q8K => "q8_K",
            Self::IQ2_XXS => "iq2_xxs",
            Self::IQ2_XS => "iq2_xs",
            Self::IQ3_XXS => "iq3_xxs",
            Self::IQ1_S => "iq1_s",
            Self::IQ4_NL => "iq4_nl",
            Self::IQ3_S => "iq3_s",
            Self::IQ2_S => "iq2_s",
            Self::IQ4_XS => "iq4_xs",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F64 => "f64",
            Self::IQ1_M => "iq1_m",
            Self::BF16 => "bf16",
            Self::TQ1_0 => "tq1_0",
            Self::TQ2_0 => "tq2_0",
            Self::MXFP4 => "mxfp4",
            Self::NVFP4 => "nvfp4",
            Self::Q1_0 => "q1_0",
            Self::Q2_0 => "q2_0",
            Self::F8_E4M3 => "f8_e4m3",
        }
    }

    /// `is_quantized` from the upstream `type_traits` table.
    pub fn is_quantized(self) -> bool {
        !matches!(
            self,
            Self::F32 | Self::F16 | Self::BF16 | Self::F64 | Self::F8_E4M3
                | Self::I8 | Self::I16 | Self::I32 | Self::I64
        )
    }

    /// Elements per block (`blck_size`: `QK*` constants of `ggml-common.h`).
    pub fn block_size(self) -> usize {
        match self {
            Self::F32 | Self::F16 | Self::BF16 | Self::F64 | Self::F8_E4M3
            | Self::I8 | Self::I16 | Self::I32 | Self::I64 => 1,
            Self::Q4_0 | Self::Q4_1 | Self::Q5_0 | Self::Q5_1 | Self::Q8_0 | Self::Q8_1
            | Self::IQ4_NL | Self::MXFP4 => 32,
            Self::Q2_0 | Self::NVFP4 => 64,
            Self::Q1_0 => 128,
            Self::Q2K | Self::Q3K | Self::Q4K | Self::Q5K | Self::Q6K | Self::Q8K
            | Self::IQ2_XXS | Self::IQ2_XS | Self::IQ3_XXS | Self::IQ1_S | Self::IQ3_S
            | Self::IQ2_S | Self::IQ4_XS | Self::IQ1_M | Self::TQ1_0 | Self::TQ2_0 => 256,
        }
    }

    /// Bytes per block (`sizeof(block_*)` in `ggml-common.h`).
    pub fn type_size(self) -> usize {
        match self {
            Self::I8 | Self::F8_E4M3 => 1,
            Self::F16 | Self::BF16 | Self::I16 => 2,
            Self::F32 | Self::I32 => 4,
            Self::I64 | Self::F64 => 8,
            Self::Q4_0 | Self::IQ4_NL | Self::Q1_0 | Self::Q2_0 => 18,
            Self::Q4_1 => 20,
            Self::Q5_0 => 22,
            Self::Q5_1 => 24,
            Self::Q8_0 => 34,
            Self::Q8_1 => 36,
            Self::MXFP4 => 17,
            Self::NVFP4 => 36,
            Self::Q2K => 84,
            Self::Q3K | Self::IQ3_S => 110,
            Self::Q4K => 144,
            Self::Q5K => 176,
            Self::Q6K => 210,
            Self::Q8K => 292,
            Self::IQ2_XXS | Self::TQ2_0 => 66,
            Self::IQ2_XS => 74,
            Self::IQ2_S => 82,
            Self::IQ3_XXS => 98,
            Self::IQ1_S => 50,
            Self::IQ1_M => 56,
            Self::IQ4_XS => 136,
            Self::TQ1_0 => 54,
        }
    }

    /// Bytes of one row of `ne0` elements (`ggml_row_size`); `None` if `ne0` is not a block multiple.
    pub fn row_size(self, ne0: usize) -> Option<usize> {
        let bs = self.block_size();
        (ne0 % bs == 0).then(|| ne0 / bs * self.type_size())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(name, id, blck_size, type_size)` from `ggml-org/ggml` master: ids from `include/ggml.h`,
    /// block sizes and `sizeof(block_*)` from `src/ggml-common.h` compiled with `gcc`;
    /// plain scalars from the `type_traits` table of `src/ggml.c`.
    const UPSTREAM: [(&str, u32, usize, usize); 36] = [
        ("f32", 0, 1, 4), ("f16", 1, 1, 2), ("q4_0", 2, 32, 18), ("q4_1", 3, 32, 20),
        ("q5_0", 6, 32, 22), ("q5_1", 7, 32, 24), ("q8_0", 8, 32, 34), ("q8_1", 9, 32, 36),
        ("q2_K", 10, 256, 84), ("q3_K", 11, 256, 110), ("q4_K", 12, 256, 144),
        ("q5_K", 13, 256, 176), ("q6_K", 14, 256, 210), ("q8_K", 15, 256, 292),
        ("iq2_xxs", 16, 256, 66), ("iq2_xs", 17, 256, 74), ("iq3_xxs", 18, 256, 98),
        ("iq1_s", 19, 256, 50), ("iq4_nl", 20, 32, 18), ("iq3_s", 21, 256, 110),
        ("iq2_s", 22, 256, 82), ("iq4_xs", 23, 256, 136), ("i8", 24, 1, 1), ("i16", 25, 1, 2),
        ("i32", 26, 1, 4), ("i64", 27, 1, 8), ("f64", 28, 1, 8), ("iq1_m", 29, 256, 56),
        ("bf16", 30, 1, 2), ("tq1_0", 34, 256, 54), ("tq2_0", 35, 256, 66),
        ("mxfp4", 39, 32, 17), ("nvfp4", 40, 64, 36), ("q1_0", 41, 128, 18),
        ("q2_0", 42, 64, 18), ("f8_e4m3", 43, 1, 1),
    ];

    #[test]
    fn ids_block_sizes_and_type_sizes_match_upstream() {
        assert_eq!(GgmlType::ALL.len(), UPSTREAM.len());
        for (t, (name, id, blck, size)) in GgmlType::ALL.iter().zip(UPSTREAM) {
            assert_eq!(t.name(), name);
            assert_eq!(t.id(), id, "{name} id");
            assert_eq!(t.block_size(), blck, "{name} blck_size");
            assert_eq!(t.type_size(), size, "{name} type_size");
            assert_eq!(GgmlType::from_id(id), Some(*t));
        }
    }

    #[test]
    fn removed_and_unknown_ids_are_rejected() {
        for id in [4, 5, 31, 32, 33, 36, 37, 38, 44, 1000] {
            assert_eq!(GgmlType::from_id(id), None, "id {id}");
        }
    }

    #[test]
    fn row_size_requires_block_multiple() {
        assert_eq!(GgmlType::Q4K.row_size(512), Some(288));
        assert_eq!(GgmlType::NVFP4.row_size(128), Some(72));
        assert_eq!(GgmlType::Q1_0.row_size(64), None);
        assert_eq!(GgmlType::F8_E4M3.row_size(7), Some(7));
        assert!(GgmlType::IQ4_NL.is_quantized() && !GgmlType::F8_E4M3.is_quantized());
    }
}
