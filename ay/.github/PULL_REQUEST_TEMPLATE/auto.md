## Objet

<!-- Ce que change la PR et pourquoi. Lier l'issue : Fixes #… -->

## Vérifications

- [ ] `bun run check` passe
- [ ] `cargo test --workspace` passe si du Rust est modifié
- [ ] `bun run auto:validate-facturx` passe si la facturation ou Factur-X est touchée
- [ ] Nouvelle migration : numérotée, idempotente, politiques RLS incluses
- [ ] Aucun secret, aucune donnée client réelle
