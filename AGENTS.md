# fairwind — NHITL Agent Contract (`agy` CLI)

- Runtime: Python (`uv` + `uv.lock`, `pyproject.toml`, `ruff`, `ty`). Never use bare `pip` or global `python`.
- Verification gate (mandatory before commit/push):
  - `uv run ruff check .`
  - `uv run ruff format --check .`
  - `uv run ty check`
- Headless `agy` CLI loop: `agy -p "<task>" --output-format stream-json` or bidirectional `--input-format stream-json --output-format stream-json`.
- Work on `main`; commit by path; zero AI attribution trailers; keep repository private (`aphrody-labs/fairwind`).
