# Lot: unify everything outside the code of aphrody-labs

Order of the owner, 2026-11-21: everything in the aphrody-labs organisation that is not code, ignored until now, is unified.

## Scope

Every repository of the org: `gh repo list aphrody-labs --limit 1000`, forks included. For each one:

- **Repository metadata:** description, topics, homepage, default branch, visibility, archive state.
- **Settings:**
  - merge options and rulesets (main is the only work branch; `release/**` is the release train, see ay `docs/release/model.md`);
  - Dependabot and security settings;
  - Actions permissions.
- **Files:** README, LICENSE and NOTICE, SECURITY, CONTRIBUTING, CODE_OF_CONDUCT, issue and PR templates, labels.
- **Pages:** wikis and GitHub Pages.
- **Org-level:**
  - the profile and `.github` repository, teams;
  - secrets and their visibility (names only, never values);
  - GitHub Apps;
  - Packages: npm, GHCR, Maven.
- **Brand:** one identity from each product's `product.toml` [brand] and ay `brand/`, the same on every repo and package page.

## Method

1. **One declarative file:** the target state goes into one file, for example `org.toml` in fw next to `product.toml`, derived from code and manifests, never hand-typed per repo.
2. **Apply and check:** `fw org plan|apply|check` diffs the live org (`gh api`) against the file and applies it. `check` runs in CI on a schedule.
3. **Report:** one table per repo, live state against target, with every change applied.

## Rules

- Forks keep their upstream LICENSE and authorship.
- No private data in public repos: no PC, VPS or account details, and hosts named by role.
- Deletions and archiving go through the plan file and are listed before they are applied.
