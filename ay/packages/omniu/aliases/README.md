# Alias Assets

This directory owns first-party alias tables. The framework loads
`aliases/<pack>.toml` for a plugin through `niubash_load_alias_asset`
(`lib/aliases.niu`); a missing file is skipped.

Each file is named after the exporting pack:

```text
aliases/git.toml
aliases/docker.toml
aliases/kubectl.toml
aliases/npm.toml
```

File format:

```toml
[aliases]
gst = 'git status'
```
