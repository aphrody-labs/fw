from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def curate_sft_dataset(
    journal_path: Path,
    out_path: Path,
    max_records: int = 2000,
) -> dict[str, Any]:
    """Build a deterministic chat/SFT JSONL dataset from Fw & ~/.gemini learning journals."""
    records: list[dict[str, Any]] = []
    if journal_path.is_file():
        for raw in journal_path.read_text(encoding="utf-8", errors="ignore").splitlines():
            if len(records) >= max_records:
                break
            line = raw.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
            except json.JSONDecodeError:
                continue
            prompt = str(ev.get("prompt") or ev.get("summary") or ev.get("tool") or "").strip()
            completion = str(ev.get("recovery") or ev.get("rule") or ev.get("result") or "").strip()
            if prompt and completion:
                records.append(
                    {
                        "messages": [
                            {"role": "system", "content": "Fw autonomous engineering assistant."},
                            {"role": "user", "content": prompt[:2000]},
                            {"role": "assistant", "content": completion[:2000]},
                        ]
                    }
                )
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")
    return {
        "ok": True,
        "journalPath": str(journal_path),
        "outPath": str(out_path),
        "records": len(records),
    }


def build_lora_plan(
    base_model: str,
    dataset_path: str,
    output_dir: str,
    rank: int = 16,
    alpha: int = 32,
    qlora_4bit: bool = True,
) -> dict[str, Any]:
    """Generate a reproducible PEFT LoRA/QLoRA + Heretic fine-tuning specification."""
    available: dict[str, bool] = {}
    for mod in ("torch", "transformers", "peft", "accelerate", "bitsandbytes", "optuna"):
        try:
            __import__(mod)
            available[mod] = True
        except Exception:
            available[mod] = False
    return {
        "ok": True,
        "baseModel": base_model,
        "datasetPath": dataset_path,
        "outputDir": output_dir,
        "lora": {"r": rank, "lora_alpha": alpha, "lora_dropout": 0.05, "qlora4bit": qlora_4bit},
        "hereticBridge": "uv run heretic --help",
        "installedMlPackages": available,
    }


def pyghidra_status(repo_root: Path) -> dict[str, Any]:
    ghidra_dir = repo_root / "packages" / "ghidra"
    pyghidra_installed = False
    try:
        __import__("pyghidra")
        pyghidra_installed = True
    except Exception:
        pyghidra_installed = False
    return {
        "ok": True,
        "ghidraDir": str(ghidra_dir),
        "ghidraPresent": ghidra_dir.is_dir(),
        "pyghidraInstalled": pyghidra_installed,
        "headlessRunner": str(ghidra_dir / "support" / "analyzeHeadless"),
    }


def magika_identify(target: Path) -> dict[str, Any]:
    """Identify file via Magika if installed, with deterministic magic/header fallback."""
    try:
        from magika import Magika  # type: ignore[import-not-found]

        m = Magika()
        res = m.identify_path(target)
        return {
            "ok": True,
            "engine": "magika",
            "path": str(target),
            "label": res.output.ct_label,
            "mime": res.output.mime_type,
            "score": float(res.output.score),
        }
    except Exception:
        head = target.read_bytes()[:64] if target.is_file() else b""
        if head.startswith(b"MZ"):
            label, mime = "pebin", "application/vnd.microsoft.portable-executable"
        elif head.startswith(b"\x7fELF"):
            label, mime = "elf", "application/x-executable"
        elif head.startswith(b"\x00asm"):
            label, mime = "wasm", "application/wasm"
        elif head.startswith(b"GGUF"):
            label, mime = "gguf", "application/x-gguf"
        else:
            label, mime = "generic", "application/octet-stream"
        return {
            "ok": True,
            "engine": "fw-magic-fallback",
            "path": str(target),
            "label": label,
            "mime": mime,
        }


if __name__ == "__main__":
    import sys

    action = sys.argv[1] if len(sys.argv) > 1 else "plan"
    if action == "dataset":
        j = Path(sys.argv[2]) if len(sys.argv) > 2 else Path.home() / ".gemini" / "learning" / "journal.jsonl"
        o = Path(sys.argv[3]) if len(sys.argv) > 3 else Path("target/datasets/fw-sft.jsonl")
        print(json.dumps(curate_sft_dataset(j, o), indent=2))
    elif action == "pyghidra":
        r = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(".")
        print(json.dumps(pyghidra_status(r), indent=2))
    elif action == "magika" and len(sys.argv) > 2:
        print(json.dumps(magika_identify(Path(sys.argv[2])), indent=2))
    else:
        model = sys.argv[2] if len(sys.argv) > 2 else "Qwen/Qwen3-4B"
        ds = sys.argv[3] if len(sys.argv) > 3 else "target/datasets/fw-sft.jsonl"
        out = sys.argv[4] if len(sys.argv) > 4 else "target/models/fw-lora"
        print(json.dumps(build_lora_plan(model, ds, out), indent=2))
