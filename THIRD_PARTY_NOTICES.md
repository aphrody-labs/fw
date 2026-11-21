# Third-Party Notices

Fw is licensed under the MIT License (see [LICENSE](./LICENSE)).
The following first-party integrated modules preserve attribution for their upstream open-source origins:

Upstream code lives in forks and registries, not in this tree:

- `heretic` (`p-e-w/heretic`, AGPL-3.0-or-later) is consumed as the PyPI package `heretic-llm` (`uvx --from heretic-llm heretic`, optional extra `heretic` in `pyproject.toml`); its presets are in `configs/heretic/`. The MIT-licensed fw tree contains no copy of it.
- `google-antigravity` (Apache-2.0) is the PyPI package; it is not vendored.
- `microsoft/intelligent-terminal` (`wta`, MIT) lives in the `aphrody-labs/intelligent-terminal` fork, owned by `fw-nt`.
- `microsoft/tui-test` (MIT) lives in the `aphrody-labs/tui-test` fork.
- `sxyazi/yazi` (MIT, Copyright (c) 2023 - sxyazi) lives in the `aphrody-labs/yazi` fork. `crates/fw-files` depends on its `yazi-sftp` crate at tag `v26.9.1` and ships `crates/fw-files/presets/yazi-plugin.toml`, a verbatim excerpt of `yazi-config/preset/yazi-default.toml` at that tag; its sort, task-queue and MIME logic are written for fw after yazi's design.
