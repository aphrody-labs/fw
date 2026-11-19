"""Zero-copy memoryview/DLPack tensor descriptors, GGUF v3 header codec, and Q8_0 parity.

Designed for byte-for-byte parity with ``crates/fw-ggml/src/lib.rs`` using Python 3.12+
standard library primitives only (zero external runtime dependencies).
"""

from __future__ import annotations

import array
import ctypes
import math
import struct
from collections.abc import Buffer, Sequence
from dataclasses import dataclass
from enum import IntEnum
from typing import Any

GGUF_MAGIC: bytes = b"GGUF"
GGUF_HEADER_BYTES: int = 24
GGUF_DEFAULT_ALIGNMENT: int = 32
Q8_0_BLOCK_SIZE: int = 32


class GgmlType(IntEnum):
    """GGML tensor data types matching ``enum ggml_type`` in ``crates/fw-ggml``."""

    F32 = 0
    F16 = 1
    Q4_0 = 2
    Q4_1 = 3
    Q5_0 = 6
    Q5_1 = 7
    Q8_0 = 8
    Q8_1 = 9
    Q2K = 10
    Q3K = 11
    Q4K = 12
    Q5K = 13
    Q6K = 14
    Q8K = 15
    BF16 = 30

    @property
    def block_size(self) -> int:
        if self in (GgmlType.F32, GgmlType.F16, GgmlType.BF16):
            return 1
        if self in (
            GgmlType.Q4_0,
            GgmlType.Q4_1,
            GgmlType.Q5_0,
            GgmlType.Q5_1,
            GgmlType.Q8_0,
            GgmlType.Q8_1,
        ):
            return 32
        return 256

    @property
    def type_size(self) -> int:
        sizes = {
            GgmlType.F32: 4,
            GgmlType.F16: 2,
            GgmlType.BF16: 2,
            GgmlType.Q4_0: 18,
            GgmlType.Q4_1: 20,
            GgmlType.Q5_0: 22,
            GgmlType.Q5_1: 24,
            GgmlType.Q8_0: 34,
            GgmlType.Q8_1: 36,
            GgmlType.Q4K: 144,
            GgmlType.Q6K: 210,
        }
        return sizes.get(self, 32)


class DLDeviceType(IntEnum):
    """Standard DLPack ``DLDeviceType`` enum values."""

    CPU = 1
    CUDA = 2
    CUDA_HOST = 3
    OPENCL = 4
    VULKAN = 7
    METAL = 8
    VPI = 9
    ROCM = 10
    CUDA_MANAGED = 13
    WEBGPU = 15


class DLDataTypeCode(IntEnum):
    """Standard DLPack ``DLDataTypeCode`` enum values."""

    INT = 0
    UINT = 1
    FLOAT = 2
    BFLOAT = 4


_DTYPE_SPECS: dict[str, tuple[DLDataTypeCode, int, int, str]] = {
    "float32": (DLDataTypeCode.FLOAT, 32, 1, "f"),
    "float64": (DLDataTypeCode.FLOAT, 64, 1, "d"),
    "float16": (DLDataTypeCode.FLOAT, 16, 1, "e"),
    "bfloat16": (DLDataTypeCode.BFLOAT, 16, 1, "H"),
    "int8": (DLDataTypeCode.INT, 8, 1, "b"),
    "uint8": (DLDataTypeCode.UINT, 8, 1, "B"),
    "int16": (DLDataTypeCode.INT, 16, 1, "h"),
    "int32": (DLDataTypeCode.INT, 32, 1, "i"),
    "int64": (DLDataTypeCode.INT, 64, 1, "q"),
}


def _c_contiguous_strides(shape: tuple[int, ...]) -> tuple[int, ...]:
    """Compute element-unit C-contiguous strides matching DLPack / PyTorch."""
    if not shape:
        return ()
    strides = [1] * len(shape)
    running = 1
    for i in range(len(shape) - 1, -1, -1):
        strides[i] = running
        running *= shape[i]
    return tuple(strides)


@dataclass(frozen=True, slots=True)
class DLPackDescriptor:
    """DLPack-compatible tensor descriptor for PyTorch/Triton/vLLM zero-copy interop."""

    dtype: str
    shape: tuple[int, ...]
    strides: tuple[int, ...]
    device_type: DLDeviceType
    byte_offset: int
    data_ptr_len: int
    device_id: int = 0

    def dlpack_dtype_tuple(self) -> tuple[int, int, int]:
        """Return ``(code, bits, lanes)`` matching ``DLDataType``."""
        code, bits, lanes, _ = _DTYPE_SPECS[self.dtype]
        return (int(code), bits, lanes)

    @property
    def itemsize(self) -> int:
        _, bits, lanes, _ = _DTYPE_SPECS[self.dtype]
        return (bits // 8) * lanes

    @property
    def nelements(self) -> int:
        return math.prod(self.shape) if self.shape else 1

    def is_contiguous(self) -> bool:
        return self.strides == _c_contiguous_strides(self.shape)

    def to_dict(self) -> dict[str, Any]:
        return {
            "dtype": self.dtype,
            "dlpack_dtype": self.dlpack_dtype_tuple(),
            "shape": self.shape,
            "strides": self.strides,
            "device_type": int(self.device_type),
            "device_name": self.device_type.name,
            "device_id": self.device_id,
            "byte_offset": self.byte_offset,
            "data_ptr_len": self.data_ptr_len,
        }


class FwTensorView:
    """Zero-copy ``memoryview`` tensor wrapper with DLPack metadata."""

    __slots__ = ("_descriptor", "_raw_view", "_view")

    def __init__(
        self,
        buffer: Buffer,
        *,
        dtype: str = "float32",
        shape: Sequence[int],
        strides: Sequence[int] | None = None,
        device_type: DLDeviceType = DLDeviceType.CPU,
        device_id: int = 0,
        byte_offset: int = 0,
    ) -> None:
        if dtype not in _DTYPE_SPECS:
            raise ValueError(f"Unsupported dtype: {dtype!r}")
        shape_tuple = tuple(int(dim) for dim in shape)
        if any(dim < 0 for dim in shape_tuple):
            raise ValueError(f"Negative dimension in shape: {shape_tuple}")
        strides_tuple = (
            _c_contiguous_strides(shape_tuple)
            if strides is None
            else tuple(int(s) for s in strides)
        )
        if len(strides_tuple) != len(shape_tuple):
            raise ValueError("strides rank must match shape rank")
        if byte_offset < 0:
            raise ValueError("byte_offset must be non-negative")

        raw = memoryview(buffer).cast("B")
        _, bits, lanes, _ = _DTYPE_SPECS[dtype]
        itemsize = (bits // 8) * lanes

        if shape_tuple and all(dim > 0 for dim in shape_tuple):
            max_elem_offset = sum(
                (dim - 1) * stride
                for dim, stride in zip(shape_tuple, strides_tuple, strict=True)
            )
            required_bytes = byte_offset + (max_elem_offset + 1) * itemsize
        else:
            required_bytes = byte_offset

        if required_bytes > raw.nbytes:
            raise ValueError(
                f"Buffer too small: need {required_bytes} bytes, got {raw.nbytes}"
            )

        self._raw_view = raw
        self._view = raw[byte_offset:]
        self._descriptor = DLPackDescriptor(
            dtype=dtype,
            shape=shape_tuple,
            strides=strides_tuple,
            device_type=device_type,
            device_id=device_id,
            byte_offset=byte_offset,
            data_ptr_len=self._view.nbytes,
        )

    @classmethod
    def from_f32_sequence(
        cls,
        values: Sequence[float],
        *,
        shape: Sequence[int] | None = None,
        device_type: DLDeviceType = DLDeviceType.CPU,
        device_id: int = 0,
    ) -> FwTensorView:
        buf = array.array("f", values)
        resolved_shape = (len(values),) if shape is None else tuple(shape)
        return cls(
            buf,
            dtype="float32",
            shape=resolved_shape,
            device_type=device_type,
            device_id=device_id,
        )

    @property
    def view(self) -> memoryview:
        """Zero-copy byte ``memoryview`` starting at ``byte_offset``."""
        return self._view

    @property
    def descriptor(self) -> DLPackDescriptor:
        return self._descriptor

    @property
    def dtype(self) -> str:
        return self._descriptor.dtype

    @property
    def shape(self) -> tuple[int, ...]:
        return self._descriptor.shape

    @property
    def strides(self) -> tuple[int, ...]:
        return self._descriptor.strides

    @property
    def device_type(self) -> DLDeviceType:
        return self._descriptor.device_type

    @property
    def byte_offset(self) -> int:
        return self._descriptor.byte_offset

    @property
    def data_ptr_len(self) -> int:
        return self._descriptor.data_ptr_len

    @property
    def nbytes(self) -> int:
        return self._descriptor.nelements * self._descriptor.itemsize

    @property
    def nelements(self) -> int:
        return self._descriptor.nelements

    @property
    def readonly(self) -> bool:
        return self._view.readonly

    def data_ptr_address(self) -> int | None:
        """Return raw C buffer address when backed by a writable contiguous buffer."""
        if self._view.readonly or self._view.nbytes == 0:
            return None
        c_buf = (ctypes.c_uint8 * self._view.nbytes).from_buffer(self._view)
        return ctypes.addressof(c_buf)

    def __dlpack_device__(self) -> tuple[int, int]:
        return (int(self._descriptor.device_type), self._descriptor.device_id)

    def slice_dim0(self, start: int, stop: int) -> FwTensorView:
        """Return a zero-copy sub-view sliced along dimension 0."""
        if not self.shape:
            raise ValueError("Cannot slice a 0-D scalar tensor view")
        dim0 = self.shape[0]
        if not (0 <= start <= stop <= dim0):
            raise IndexError(f"Invalid slice [{start}:{stop}] for dim0={dim0}")
        new_offset = self.byte_offset + start * self.strides[0] * self._descriptor.itemsize
        new_shape = (stop - start, *self.shape[1:])
        return FwTensorView(
            self._raw_view,
            dtype=self.dtype,
            shape=new_shape,
            strides=self.strides,
            device_type=self.device_type,
            device_id=self._descriptor.device_id,
            byte_offset=new_offset,
        )

    def as_f32_list(self) -> list[float]:
        """Read a contiguous ``float32`` view as Python floats."""
        if self.dtype != "float32":
            raise TypeError(f"Expected float32 view, got {self.dtype!r}")
        if not self._descriptor.is_contiguous():
            raise ValueError("Non-contiguous tensor view cannot be unpacked flat")
        count = self.nelements
        if count == 0:
            return []
        return list(struct.unpack_from(f"<{count}f", self._view, 0))

    def write_f32_at(self, flat_index: int, value: float) -> None:
        """In-place write into a writable ``float32`` view (proves zero-copy sharing)."""
        if self.dtype != "float32":
            raise TypeError(f"Expected float32 view, got {self.dtype!r}")
        if self.readonly:
            raise TypeError("Cannot mutate read-only tensor view")
        if not (0 <= flat_index < self.nelements):
            raise IndexError(f"Index {flat_index} out of range [0, {self.nelements})")
        struct.pack_into("<f", self._view, flat_index * 4, float(value))


@dataclass(frozen=True, slots=True)
class GgufHeader:
    """GGUF v2/v3 binary header matching ``crates/fw-ggml::GgufHeader``."""

    version: int
    tensor_count: int
    metadata_kv_count: int

    @property
    def kv_count(self) -> int:
        return self.metadata_kv_count


def encode_gguf_header(version: int, tensor_count: int, kv_count: int) -> bytes:
    """Encode a 24-byte little-endian GGUF header matching ``fw_ggml::encode_gguf_header``."""
    if version < 0 or version > 0xFFFFFFFF:
        raise ValueError(f"GGUF version out of u32 range: {version}")
    if tensor_count < 0 or kv_count < 0:
        raise ValueError("tensor_count and kv_count must be non-negative")
    return struct.pack("<4sIQQ", GGUF_MAGIC, version, tensor_count, kv_count)


def parse_gguf_header(data: bytes | bytearray | memoryview) -> GgufHeader:
    """Decode a 24-byte GGUF header matching ``fw_ggml::parse_gguf_header``."""
    view = memoryview(data)
    if len(view) < GGUF_HEADER_BYTES:
        raise ValueError("GGUF header too short (<24 bytes)")
    magic, version, tensor_count, metadata_kv_count = struct.unpack_from("<4sIQQ", view, 0)
    if magic != GGUF_MAGIC:
        raise ValueError("Invalid GGUF magic")
    if version not in (2, 3):
        raise ValueError(f"Unsupported GGUF version {version}")
    return GgufHeader(
        version=version,
        tensor_count=tensor_count,
        metadata_kv_count=metadata_kv_count,
    )


def _round_half_away_from_zero(value: float) -> int:
    """Match Rust ``f32::round()`` (round half away from zero)."""
    return math.floor(value + 0.5) if value >= 0.0 else math.ceil(value - 0.5)


def _to_f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


@dataclass(frozen=True, slots=True)
class Q8_0Block:
    """One Q8_0 quantized block (up to 32 ``int8`` values + ``f32`` scale).

    Matches ``fw_ggml::quantize_q8_0`` scale convention: ``scale = max(|x|) / 127.0``
    (or ``1.0`` when all elements are zero).
    """

    scale: float
    quants: tuple[int, ...]
    original_len: int = Q8_0_BLOCK_SIZE

    def to_bytes(self) -> bytes:
        """Pack as ``<f32b`` (4-byte f32 scale + 32 signed int8 quants = 36 bytes)."""
        padded = self.quants + (0,) * (Q8_0_BLOCK_SIZE - len(self.quants))
        return struct.pack("<f32b", self.scale, *padded[:Q8_0_BLOCK_SIZE])

    @classmethod
    def from_bytes(
        cls, data: bytes | memoryview, original_len: int = Q8_0_BLOCK_SIZE
    ) -> Q8_0Block:
        scale, *qs = struct.unpack_from("<f32b", data, 0)
        return cls(scale=scale, quants=tuple(qs[:original_len]), original_len=original_len)


def quantize_q8_0_block(values: Sequence[float]) -> Q8_0Block:
    """Quantize a single block (<= 32 floats) matching ``fw_ggml::quantize_q8_0``."""
    if len(values) > Q8_0_BLOCK_SIZE:
        raise ValueError(f"Q8_0 block accepts at most {Q8_0_BLOCK_SIZE} elements")
    f32_vals = [_to_f32(float(x)) for x in values]
    amax = max((abs(x) for x in f32_vals), default=0.0)
    scale = 1.0 if amax == 0.0 else _to_f32(amax / 127.0)
    inv = _to_f32(1.0 / scale)
    quants = tuple(
        max(-127, min(127, _round_half_away_from_zero(_to_f32(x * inv))))
        for x in f32_vals
    )
    return Q8_0Block(scale=scale, quants=quants, original_len=len(f32_vals))


def quantize_q8_0(values: Sequence[float]) -> list[Q8_0Block]:
    """Quantize an arbitrary-length float sequence into 32-element Q8_0 blocks."""
    if not values:
        return []
    blocks: list[Q8_0Block] = []
    for start in range(0, len(values), Q8_0_BLOCK_SIZE):
        chunk = values[start : start + Q8_0_BLOCK_SIZE]
        blocks.append(quantize_q8_0_block(chunk))
    return blocks


def dequantize_q8_0(blocks: Sequence[Q8_0Block]) -> list[float]:
    """Dequantize Q8_0 blocks back to ``f32`` floats."""
    out: list[float] = []
    for block in blocks:
        s = block.scale
        for q in block.quants[: block.original_len]:
            out.append(_to_f32(q * s))
    return out


def dot_q8_0(blocks_a: Sequence[Q8_0Block], blocks_b: Sequence[Q8_0Block]) -> float:
    """Compute quantized Q8_0 dot product matching ``fw_ggml::dot_q8_0``."""
    if len(blocks_a) != len(blocks_b):
        raise ValueError(f"Block count mismatch: {len(blocks_a)} vs {len(blocks_b)}")
    total = 0.0
    for ba, bb in zip(blocks_a, blocks_b, strict=True):
        if len(ba.quants) != len(bb.quants):
            raise ValueError(
                f"Block length mismatch: {len(ba.quants)} vs {len(bb.quants)}"
            )
        acc = sum(a * b for a, b in zip(ba.quants, bb.quants, strict=True))
        total += _to_f32(float(acc) * ba.scale * bb.scale)
    return _to_f32(total)
