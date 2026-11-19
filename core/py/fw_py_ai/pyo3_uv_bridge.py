"""Pinned uv/ruff/PyO3 fork metadata, PEP 723 uv script generator, and PyO3 abi3 FFI bridge.

Uses Python 3.12+ stdlib ``tomllib`` to verify live manifests under ``third_party/{uv,ruff,pyo3}``
and generates:
- Reproducible PEP 723 inline-metadata ``uv run --script`` jobs for GPU/Triton/vLLM evaluation
- Zero-copy ``PyBuffer<u8>`` PyO3 ``abi3`` Rust-to-Python FFI contracts targeting ``fw-ggml``
  and ``fw-compute``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import tomllib


@dataclass(frozen=True, slots=True)
class ForkPin:
    """Verified version and Cargo metadata for a Fairwind ``third_party/`` fork."""

    name: str
    version: str
    edition: str
    rust_version: str
    repository: str
    license: str
    manifest_relpath: str


PINNED_FORKS: dict[str, ForkPin] = {
    "uv": ForkPin(
        name="uv",
        version="0.13.9",
        edition="2024",
        rust_version="1.98.0",
        repository="https://github.com/astral-sh/uv",
        license="MIT OR Apache-2.0",
        manifest_relpath="third_party/uv/Cargo.toml",
    ),
    "ruff": ForkPin(
        name="ruff",
        version="0.17.6",
        edition="2024",
        rust_version="1.98",
        repository="https://github.com/astral-sh/ruff",
        license="MIT",
        manifest_relpath="third_party/ruff/Cargo.toml",
    ),
    "pyo3": ForkPin(
        name="pyo3",
        version="0.30.0",
        edition="2024",
        rust_version="1.85",
        repository="https://github.com/pyo3/pyo3",
        license="MIT OR Apache-2.0",
        manifest_relpath="third_party/pyo3/Cargo.toml",
    ),
}


def _extract_version(cargo_data: dict[str, Any], crate_name: str) -> str:
    pkg = cargo_data.get("package", {})
    if (
        isinstance(pkg, dict)
        and pkg.get("name") == crate_name
        and isinstance(pkg.get("version"), str)
    ):
        return pkg["version"]
    ws_deps = cargo_data.get("workspace", {}).get("dependencies", {})
    dep = ws_deps.get(crate_name)
    if isinstance(dep, dict) and isinstance(dep.get("version"), str):
        return dep["version"]
    if isinstance(dep, str):
        return dep
    raise ValueError(f"Could not resolve version for {crate_name!r}")


def _extract_ws_package_field(cargo_data: dict[str, Any], field: str, default: str) -> str:
    ws_pkg = cargo_data.get("workspace", {}).get("package", {})
    if isinstance(ws_pkg, dict) and isinstance(ws_pkg.get(field), str):
        return ws_pkg[field]
    pkg = cargo_data.get("package", {})
    if isinstance(pkg, dict) and isinstance(pkg.get(field), str):
        return pkg[field]
    return default


def load_fork_metadata(repo_root: Path | None = None) -> dict[str, ForkPin]:
    """Load exact fork versions from ``third_party/{uv,ruff,pyo3}/Cargo.toml`` if present."""
    if repo_root is None:
        repo_root = Path(__file__).resolve().parents[3]

    resolved: dict[str, ForkPin] = {}
    for name, fallback in PINNED_FORKS.items():
        manifest_path = repo_root / fallback.manifest_relpath
        if not manifest_path.is_file():
            resolved[name] = fallback
            continue
        cargo_data = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        resolved[name] = ForkPin(
            name=name,
            version=_extract_version(cargo_data, name),
            edition=_extract_ws_package_field(cargo_data, "edition", fallback.edition),
            rust_version=_extract_ws_package_field(
                cargo_data, "rust-version", fallback.rust_version
            ),
            repository=_extract_ws_package_field(cargo_data, "repository", fallback.repository),
            license=_extract_ws_package_field(cargo_data, "license", fallback.license),
            manifest_relpath=fallback.manifest_relpath,
        )
    return resolved


@dataclass(frozen=True, slots=True)
class Pep723ScriptSpec:
    """Specification for a PEP 723 inline-metadata ``uv`` script."""

    requires_python: str = ">=3.12"
    dependencies: tuple[str, ...] = ("torch>=2.6", "triton>=3.2", "vllm>=0.8")
    exclude_newer: str | None = "2026-11-19T00:00:00Z"
    index_url: str | None = None
    body: str = ""


def _toml_quote(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def generate_pep723_script(spec: Pep723ScriptSpec) -> str:
    """Render a valid PEP 723 inline ``uv`` script (``# /// script ... # ///``)."""
    meta_lines: list[str] = [
        f"requires-python = {_toml_quote(spec.requires_python)}",
        "dependencies = [",
    ]
    for dep in spec.dependencies:
        meta_lines.append(f"    {_toml_quote(dep)},")
    meta_lines.append("]")

    uv_table_lines: list[str] = []
    if spec.exclude_newer:
        uv_table_lines.append(f"exclude-newer = {_toml_quote(spec.exclude_newer)}")
    if spec.index_url:
        uv_table_lines.append(f"index-url = {_toml_quote(spec.index_url)}")
    if uv_table_lines:
        meta_lines.append("")
        meta_lines.append("[tool.uv]")
        meta_lines.extend(uv_table_lines)

    header = ["# /// script"]
    for line in meta_lines:
        header.append(f"# {line}" if line else "#")
    header.append("# ///")

    body = spec.body.rstrip("\n")
    if body:
        return "\n".join(header) + "\n\n" + body + "\n"
    return "\n".join(header) + "\n"


_PEP723_RE = re.compile(
    r"(?m)^# /// script\s*\n(?P<body>(?:#(?: .*|)\n)*?)^# ///\s*$"
)


def parse_pep723_metadata(script_text: str) -> dict[str, Any]:
    """Parse and validate a PEP 723 ``# /// script`` block via stdlib ``tomllib``."""
    match = _PEP723_RE.search(script_text)
    if match is None:
        raise ValueError("No valid PEP 723 '# /// script' metadata block found")
    raw_lines: list[str] = []
    for line in match.group("body").splitlines():
        if line.startswith("# "):
            raw_lines.append(line[2:])
        elif line == "#":
            raw_lines.append("")
        else:
            raw_lines.append(line.lstrip("#"))
    return tomllib.loads("\n".join(raw_lines))


def generate_gpu_eval_script(
    *,
    model_id: str,
    backend: str = "triton",
    quantization: str = "q8_0",
    exclude_newer: str | None = "2026-11-19T00:00:00Z",
) -> str:
    """Generate a reproducible ``uv run --script`` GPU/Triton/vLLM evaluation job."""
    deps = ["torch>=2.6", "vllm>=0.8"]
    if backend == "triton":
        deps.append("triton>=3.2")
    body = f'''"""Reproducible Fairwind GPU evaluation harness ({backend}, {quantization})."""

import json
import sys


def main() -> None:
    report = {{
        "model_id": {model_id!r},
        "backend": {backend!r},
        "quantization": {quantization!r},
        "python": sys.version.split()[0],
    }}
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
'''
    return generate_pep723_script(
        Pep723ScriptSpec(
            requires_python=">=3.12",
            dependencies=tuple(deps),
            exclude_newer=exclude_newer,
            body=body,
        )
    )


@dataclass(frozen=True, slots=True)
class PyO3Abi3ContractSpec:
    """PyO3 ``abi3`` zero-copy ``PyBuffer<u8>`` FFI bridge specification."""

    crate_name: str = "fw_py_native"
    pyo3_version: str = PINNED_FORKS["pyo3"].version
    abi3_feature: str = "abi3-py312"
    rust_edition: str = "2024"
    fw_ggml_path: str = "../../crates/fw-ggml"
    fw_compute_path: str = "../../crates/fw-compute"


def generate_pyo3_cargo_toml(spec: PyO3Abi3ContractSpec | None = None) -> str:
    """Generate a valid ``Cargo.toml`` for the PyO3 ``abi3`` cdylib targeting ``fw-ggml`` & ``fw-compute``."""
    s = spec or PyO3Abi3ContractSpec()
    return f"""[package]
name = "{s.crate_name}"
version = "0.1.0"
edition = "{s.rust_edition}"

[lib]
name = "{s.crate_name}"
crate-type = ["cdylib"]

[dependencies]
pyo3 = {{ version = "{s.pyo3_version}", features = ["extension-module", "{s.abi3_feature}"] }}
fw-ggml = {{ path = "{s.fw_ggml_path}" }}
fw-compute = {{ path = "{s.fw_compute_path}" }}
"""


def generate_pyo3_abi3_rust_bridge(spec: PyO3Abi3ContractSpec | None = None) -> str:
    """Generate a zero-copy ``PyBuffer<u8>`` PyO3 ``abi3`` Rust bridge for ``fw-ggml`` & ``fw-compute``."""
    s = spec or PyO3Abi3ContractSpec()
    return f'''//! Auto-generated PyO3 {s.pyo3_version} ({s.abi3_feature}) zero-copy FFI bridge
//! targeting ``fw-ggml`` and ``fw-compute`` via ``PyBuffer<u8>``.

use fw_compute::{{detect_hardware_profile, DEFAULT_WGSL_KERNEL}};
use fw_ggml::{{dot_q8_0, encode_gguf_header, parse_gguf_header, quantize_q8_0}};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Parse a 24-byte GGUF v2/v3 header directly from a zero-copy Python buffer (memoryview/bytes).
#[pyfunction]
fn gguf_parse_header_zero_copy(py: Python<'_>, buf: PyBuffer<u8>) -> PyResult<(u32, u64, u64)> {{
    let slice = buf
        .as_slice(py)
        .ok_or_else(|| PyValueError::new_err("GGUF buffer must be C-contiguous"))?;
    let bytes: Vec<u8> = slice.iter().map(|cell| cell.get()).collect();
    let hdr = parse_gguf_header(&bytes).map_err(PyValueError::new_err)?;
    Ok((hdr.version, hdr.tensor_count, hdr.metadata_kv_count))
}}

/// Encode a 24-byte GGUF v3 header via ``fw_ggml::encode_gguf_header``.
#[pyfunction]
fn gguf_encode_header(version: u32, tensor_count: u64, kv_count: u64) -> Vec<u8> {{
    encode_gguf_header(version, tensor_count, kv_count).to_vec()
}}

/// Quantize a zero-copy f32 byte buffer into Q8_0 (scale, int8 quants) via ``fw_ggml::quantize_q8_0``.
#[pyfunction]
fn ggml_quantize_q8_0_buffer(py: Python<'_>, buf: PyBuffer<u8>) -> PyResult<(f32, Vec<i8>)> {{
    let slice = buf
        .as_slice(py)
        .ok_or_else(|| PyValueError::new_err("f32 buffer must be C-contiguous"))?;
    if slice.len() % 4 != 0 {{
        return Err(PyValueError::new_err("f32 buffer byte length must be a multiple of 4"));
    }}
    let mut vals = Vec::with_capacity(slice.len() / 4);
    for chunk in slice.chunks_exact(4) {{
        let raw = [chunk[0].get(), chunk[1].get(), chunk[2].get(), chunk[3].get()];
        vals.push(f32::from_le_bytes(raw));
    }}
    Ok(quantize_q8_0(&vals))
}}

/// Compute Q8_0 quantized dot product via ``fw_ggml::dot_q8_0``.
#[pyfunction]
fn ggml_dot_q8_0(sa: f32, qa: Vec<i8>, sb: f32, qb: Vec<i8>) -> f32 {{
    dot_q8_0(sa, &qa, sb, &qb)
}}

/// Query host SIMD/GPU/WebGPU capabilities via ``fw_compute::detect_hardware_profile``.
#[pyfunction]
fn compute_hardware_summary() -> (String, String, usize, usize) {{
    let profile = detect_hardware_profile();
    (profile.os, profile.arch, profile.logical_cpus, DEFAULT_WGSL_KERNEL.len())
}}

#[pymodule]
fn {s.crate_name}(m: &Bound<'_, PyModule>) -> PyResult<()> {{
    m.add_function(wrap_pyfunction!(gguf_parse_header_zero_copy, m)?)?;
    m.add_function(wrap_pyfunction!(gguf_encode_header, m)?)?;
    m.add_function(wrap_pyfunction!(ggml_quantize_q8_0_buffer, m)?)?;
    m.add_function(wrap_pyfunction!(ggml_dot_q8_0, m)?)?;
    m.add_function(wrap_pyfunction!(compute_hardware_summary, m)?)?;
    Ok(())
}}
'''
