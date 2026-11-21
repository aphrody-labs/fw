# Contribuer

## Environnement

- Runtime [Aphrody](https://aphrody.app) (fork de Bun) : `bun check`, `bun lint` et `bun fmt` sont natifs.
- Base de dev : SQLite locale ; migrations avec `bun run auto:migrate`.
- Rust : compilé et testé sur l’hôte de build (`bun tools/auto-cargo.ts`), jamais sur le PC.

## Règles

- Règles du dépôt : `AGENTS.md` à la racine (travail sur `main`, aucune attribution d'IA dans les commits).
- Les gates passent avant tout push : `bun run check` à la racine, et `cargo test --workspace` si du Rust change.
- Migrations SQL : nouveau fichier numéroté dans `packages/auto-store/migrations/`, jamais de modification d'une migration appliquée, RLS sur toute table de tenant.
- Pas de secret ni de donnée client réelle dans le dépôt, les tests ou les issues.
