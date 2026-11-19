"""Zero-dependency CLI migration engine: ``~/.hermes/`` -> Fairwind (``fw``) config & skills.

Converts Hermes Agent homes (``config.yaml``/``config.json``, ``memories/``, ``skills/``,
``cron/``) into Fairwind's normalized config, ``MemoryEntry`` JSONL index, ``agentskills.io``
``SKILL.md`` trees, and cron schedule definitions using Python 3.12+ stdlib only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import tomllib


def _parse_scalar(raw: str) -> Any:
    val = raw.strip()
    if not val:
        return ""
    if (val.startswith('"') and val.endswith('"')) or (
        val.startswith("'") and val.endswith("'")
    ):
        return val[1:-1]
    lower = val.lower()
    if lower == "true":
        return True
    if lower == "false":
        return False
    if lower in ("null", "none", "~"):
        return None
    try:
        return int(val)
    except ValueError:
        pass
    try:
        return float(val)
    except ValueError:
        return val


def parse_simple_yaml(text: str) -> dict[str, Any]:
    """Parse a practical subset of YAML (nested dicts, lists, scalars) using stdlib only."""
    root: dict[str, Any] = {}
    stack: list[tuple[int, dict[str, Any] | list[Any]]] = [(-1, root)]

    raw_lines = text.splitlines()
    for idx, raw_line in enumerate(raw_lines):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#") or stripped in ("---", "..."):
            continue
        indent = len(raw_line) - len(raw_line.lstrip(" "))

        while len(stack) > 1 and indent <= stack[-1][0]:
            stack.pop()
        container = stack[-1][1]

        if stripped.startswith("- "):
            item_str = stripped[2:].strip()
            if not isinstance(container, list):
                continue
            if ":" in item_str and not item_str.startswith(('"', "'")):
                k, _, v = item_str.partition(":")
                item_dict: dict[str, Any] = {k.strip(): _parse_scalar(v)}
                container.append(item_dict)
                stack.append((indent + 2, item_dict))
            else:
                container.append(_parse_scalar(item_str))
            continue

        if ":" not in stripped or not isinstance(container, dict):
            continue
        key, _, rest = stripped.partition(":")
        key = key.strip().strip('"').strip("'")
        val_str = rest.strip()
        if val_str:
            container[key] = _parse_scalar(val_str)
        else:
            # Peek next non-empty line to see if nested list or dict
            next_is_list = False
            for nxt in raw_lines[idx + 1 :]:
                nxt_s = nxt.strip()
                if not nxt_s or nxt_s.startswith("#"):
                    continue
                next_is_list = nxt_s.startswith("- ")
                break
            child: dict[str, Any] | list[Any] = [] if next_is_list else {}
            container[key] = child
            stack.append((indent, child))

    return root


@dataclass(frozen=True, slots=True)
class MigrationAction:
    """Single planned or executed migration operation."""

    category: str
    source: str
    target: str
    status: str  # "planned" | "written" | "skipped"
    detail: str = ""


@dataclass(slots=True)
class MigrationReport:
    """Structured summary returned by ``migrate_hermes_to_fw``."""

    source_dir: str
    target_dir: str
    dry_run: bool
    actions: list[MigrationAction] = field(default_factory=list)
    config_migrated: bool = False
    memories_migrated: int = 0
    skills_migrated: int = 0
    cron_jobs_migrated: int = 0
    warnings: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_dir": self.source_dir,
            "target_dir": self.target_dir,
            "dry_run": self.dry_run,
            "config_migrated": self.config_migrated,
            "memories_migrated": self.memories_migrated,
            "skills_migrated": self.skills_migrated,
            "cron_jobs_migrated": self.cron_jobs_migrated,
            "warnings": list(self.warnings),
            "actions": [
                {
                    "category": a.category,
                    "source": a.source,
                    "target": a.target,
                    "status": a.status,
                    "detail": a.detail,
                }
                for a in self.actions
            ],
        }


def _load_structured_file(path: Path) -> Any:
    text = path.read_text(encoding="utf-8")
    suffix = path.suffix.lower()
    if suffix == ".json":
        return json.loads(text)
    if suffix == ".toml":
        return tomllib.loads(text)
    return parse_simple_yaml(text)


def _normalize_config(raw: dict[str, Any]) -> dict[str, Any]:
    model_cfg = raw.get("model", {})
    if isinstance(model_cfg, str):
        model_cfg = {"default": model_cfg}
    return {
        "schema": "fairwind.config.v1",
        "migrated_from": "hermes-agent",
        "model": model_cfg if isinstance(model_cfg, dict) else {},
        "provider": raw.get("provider", raw.get("inference_provider", "auto")),
        "memory": raw.get("memory", {"backend": "fw-local-jsonl"}),
        "channels": raw.get("channels", ["cli", "mcp-stdio", "acp"]),
        "sandbox": raw.get("sandbox", {"read_only_default": True, "max_steps": 12}),
        "raw_hermes_keys": sorted(raw.keys()),
    }


def _ensure_skill_frontmatter(skill_name: str, content: str) -> str:
    stripped = content.lstrip()
    if stripped.startswith(("---\n", "---\r\n")):
        return content
    header = (
        "---\n"
        f"name: {skill_name}\n"
        f"description: Migrated from Hermes Agent skill ({skill_name})\n"
        "---\n\n"
    )
    return header + content


def _write_text(path: Path, content: str, *, dry_run: bool) -> str:
    if dry_run:
        return "planned"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")
    return "written"


def migrate_hermes_to_fw(
    source_dir: str | Path,
    target_dir: str | Path,
    *,
    dry_run: bool = True,
) -> MigrationReport:
    """Convert a ``~/.hermes/`` directory into Fairwind ``fw`` config, memories, skills, and cron."""
    src = Path(source_dir).expanduser()
    dst = Path(target_dir).expanduser()
    report = MigrationReport(source_dir=str(src), target_dir=str(dst), dry_run=dry_run)

    if not src.exists():
        report.warnings.append(f"Source Hermes directory does not exist: {src}")
        return report

    # 1. Config (config.json preferred if present, else config.yaml / config.yml)
    for cfg_name in ("config.json", "config.yaml", "config.yml"):
        cfg_path = src / cfg_name
        if cfg_path.is_file():
            raw_cfg = _load_structured_file(cfg_path)
            if isinstance(raw_cfg, dict):
                normalized = _normalize_config(raw_cfg)
                out_cfg = dst / "fw-config.json"
                status = _write_text(
                    out_cfg,
                    json.dumps(normalized, indent=2, sort_keys=True) + "\n",
                    dry_run=dry_run,
                )
                report.config_migrated = True
                report.actions.append(
                    MigrationAction(
                        category="config",
                        source=str(cfg_path),
                        target=str(out_cfg),
                        status=status,
                        detail=f"normalized {len(raw_cfg)} top-level keys",
                    )
                )
            break

    # 2. Memories (memories/*.md, *.jsonl, *.json, *.txt -> memories/index.jsonl)
    mem_dir = src / "memories"
    if mem_dir.is_dir():
        entries: list[dict[str, Any]] = []
        for mf in sorted(mem_dir.rglob("*")):
            if not mf.is_file():
                continue
            rel = mf.relative_to(mem_dir).as_posix()
            text = mf.read_text(encoding="utf-8", errors="replace").strip()
            if not text:
                continue
            if mf.suffix.lower() == ".jsonl":
                for line_no, line in enumerate(text.splitlines(), start=1):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        obj = json.loads(line)
                    except json.JSONDecodeError:
                        obj = {"content": line}
                    entry_id = hashlib.sha256(f"{rel}:{line_no}".encode()).hexdigest()[:16]
                    entries.append(
                        {
                            "id": str(obj.get("id", entry_id)),
                            "kind": str(obj.get("kind", "episodic")),
                            "topic": str(obj.get("topic", mf.stem)),
                            "content": str(obj.get("content", line)),
                            "tags": list(obj.get("tags", ["hermes-migrated"])),
                        }
                    )
            else:
                entry_id = hashlib.sha256(rel.encode()).hexdigest()[:16]
                entries.append(
                    {
                        "id": entry_id,
                        "kind": "semantic",
                        "topic": mf.stem,
                        "content": text,
                        "tags": ["hermes-migrated", mf.suffix.lstrip(".") or "text"],
                    }
                )

        if entries:
            out_mem = dst / "memories" / "index.jsonl"
            payload = "\n".join(json.dumps(e, sort_keys=True) for e in entries) + "\n"
            status = _write_text(out_mem, payload, dry_run=dry_run)
            report.memories_migrated = len(entries)
            report.actions.append(
                MigrationAction(
                    category="memories",
                    source=str(mem_dir),
                    target=str(out_mem),
                    status=status,
                    detail=f"{len(entries)} MemoryEntry records",
                )
            )

    # 3. Skills (skills/<name>/SKILL.md or skills/*.md -> target/skills/<name>/SKILL.md)
    skills_dir = src / "skills"
    if skills_dir.is_dir():
        for sf in sorted(skills_dir.rglob("*.md")):
            if not sf.is_file():
                continue
            if sf.name.upper() == "SKILL.MD" and sf.parent != skills_dir:
                skill_name = sf.parent.name
            else:
                skill_name = sf.stem
            content = _ensure_skill_frontmatter(
                skill_name, sf.read_text(encoding="utf-8", errors="replace")
            )
            out_skill = dst / "skills" / skill_name / "SKILL.md"
            status = _write_text(out_skill, content, dry_run=dry_run)
            report.skills_migrated += 1
            report.actions.append(
                MigrationAction(
                    category="skills",
                    source=str(sf),
                    target=str(out_skill),
                    status=status,
                    detail=f"agentskills.io SKILL.md ({skill_name})",
                )
            )

    # 4. Cron (cron/*.json, *.yaml, *.yml, *.toml -> target/cron/jobs.json)
    cron_dir = src / "cron"
    if cron_dir.is_dir():
        jobs: list[dict[str, Any]] = []
        for cf in sorted(cron_dir.rglob("*")):
            if not cf.is_file() or cf.suffix.lower() not in (".json", ".yaml", ".yml", ".toml"):
                continue
            loaded = _load_structured_file(cf)
            raw_jobs: list[dict[str, Any]] = []
            if isinstance(loaded, list):
                raw_jobs = [j for j in loaded if isinstance(j, dict)]
            elif isinstance(loaded, dict):
                if isinstance(loaded.get("jobs"), list):
                    raw_jobs = [j for j in loaded["jobs"] if isinstance(j, dict)]
                else:
                    raw_jobs = [loaded]
            for idx, job in enumerate(raw_jobs):
                jobs.append(
                    {
                        "id": str(job.get("id", job.get("name", f"{cf.stem}-{idx}"))),
                        "schedule": str(job.get("schedule", job.get("cron", "0 * * * *"))),
                        "prompt": str(job.get("prompt", job.get("command", ""))),
                        "enabled": bool(job.get("enabled", True)),
                        "source_file": cf.name,
                    }
                )

        if jobs:
            out_cron = dst / "cron" / "jobs.json"
            status = _write_text(
                out_cron,
                json.dumps({"schema": "fairwind.cron.v1", "jobs": jobs}, indent=2) + "\n",
                dry_run=dry_run,
            )
            report.cron_jobs_migrated = len(jobs)
            report.actions.append(
                MigrationAction(
                    category="cron",
                    source=str(cron_dir),
                    target=str(out_cron),
                    status=status,
                    detail=f"{len(jobs)} cron jobs",
                )
            )

    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="migrate-hermes-to-fw",
        description="Migrate ~/.hermes/ (config, memories, skills, cron) to Fairwind (fw).",
    )
    parser.add_argument("--source", default="~/.hermes", help="Source Hermes home directory")
    parser.add_argument("--target", default="~/.fw", help="Target Fairwind directory")
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Write migrated files (default is dry-run)",
    )
    args = parser.parse_args(argv)
    report = migrate_hermes_to_fw(args.source, args.target, dry_run=not args.apply)
    print(json.dumps(report.to_dict(), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
