# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Older spec pages no longer display an example changelog written to newer
  conventions than the page describes: each page's example is now pinned to
  its own version's last release, derived at build time from this file.

## [2.0.0] - 2026-06-07

2.0.0 is the first major revision of Keep a Changelog. It breaks the guidance,
not the format: the six change types, `YYYY-MM-DD` dates, and the `Unreleased`
and `[YANKED]` markers are all unchanged, so your existing changelog stays valid.
What breaks is the surface around it. The page is restructured so some older
section links no longer resolve, the recommended guidance has shifted, and
existing translations are out of date until they catch up. The breaking changes
are marked below.

### Removed

- Outdated specifics (the Vandamme gem reference and a GitHub-Releases
  discoverability note) in favor of more general guidance.
- **Breaking:** The FAQ scaffolding and most first-person framing, whose section
  anchors no longer resolve; the podcast note now lives in the References section.

### Changed

- **Breaking:** Retired the tagline "Don't let your friends dump git logs into
  changelogs" for "Clearly document the evolution of your projects." Earlier
  versions keep the original.
- **Breaking:** Restructured the page from a flat FAQ into integrated guidance,
  in a plainer, less first-person voice. Some older section links no longer resolve.
- Reframed the "GitHub Releases" answer as "Is a changelog the same as release
  notes?", broadened beyond GitHub to any host.
- Set page titles and descriptions from frontmatter, and fixed OpenGraph metadata
  so shared links render correctly and language-appropriately.

### Added

- New guidance answering long-standing community questions:
  - Format: the `# Changelog` header preamble, and adopting a newer version of
    the format without rewriting history; marking breaking changes and
    where upgrade steps belong; naming which part of the project an entry
    touches; choosing between Changed, Fixed, and Security; leading a Security
    entry with its CVE and pointing it to the full advisory; why the six change
    types don't grow, and a recommended order for them.
  - Versioning: schemes beyond SemVer, and linking each version to a `compare`
    diff with reference links.
  - Changelogs vs. release notes: how to derive one from the other without
    duplicate work, and why a host's generated notes are vendor lock-in.
  - Automation: LLM-drafted changelogs with a brief for an `AGENTS.md`;
    Conventional Commits; CI/CD; linking issues and pull requests; crediting
    contributors.
  - Scale: very large changelogs and monorepos.
  - Optional per-release summaries, and a statement of what Keep a Changelog
    deliberately won't do.
- A redesigned, accessible site (WCAG 2.1 AA) with light and dark themes.

## [1.1.2] - 2024-09-27

### Added

- v1.1 German translation.
- v1.1 Italian translation.
- v1.1 Simplified Chinese translation.
- v1.1 Persian translation.
- v1.1 Polish translation.
- v1.1 Slovenian translation.
- v1.1 Traditional Chinese translation.
- v1.1 Spanish translation.
- v1.1 Brazilian Portuguese translation.
- v1.1 Czech translation.
- v1.1 Romanian translation.
- v1.1 Swedish translation.
- v1.1 Ukrainian translation.
- v1.1 Korean translation.
- v1.1 Indonesian translation.

### Fixed

- Improve French translation.
- Improve Dutch translation.

