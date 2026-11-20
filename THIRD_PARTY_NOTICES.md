# Third-Party Notices

Fw is licensed under the MIT License (see [LICENSE](./LICENSE)).
The following first-party integrated modules preserve attribution for their upstream open-source origins:

Upstream code lives in forks and registries, not in this tree:

- `heretic` (`p-e-w/heretic`, AGPL-3.0-or-later) is consumed as the PyPI package `heretic-llm` (`uvx --from heretic-llm heretic`, optional extra `heretic` in `pyproject.toml`); its presets are in `configs/heretic/`. The MIT-licensed fw tree contains no copy of it.
- `google-antigravity` (Apache-2.0) is the PyPI package; it is not vendored.
- `microsoft/intelligent-terminal` (`wta`, MIT) lives in the `aphrody-labs/intelligent-terminal` fork, owned by `fw-nt`.
- `microsoft/tui-test` (MIT) lives in the `aphrody-labs/tui-test` fork.
