---
name: rust-ai-engineering-ecosystem
description: "Guide opérationnel complet pour développer, documenter, compiler à haute vélocité (rustd) et migrer des bases de code vers Rust avec des agents IA"
version: 1.0.0
tags:
  - rust
  - ai-agents
  - bun-rewrite
  - rustd
  - cargo
  - rustc
  - rustdoc
  - mcp
  - systems-programming
sources:
  - https://www.youtube.com/watch?v=QRnfDHVSwZI
  - https://bun.com/blog/bun-in-rust
  - https://github.com/dioxusLabs/rustd
  - https://github.com/rust-lang/rust
  - https://github.com/rust-lang/cargo
  - https://doc.rust-lang.org/rustc/index.html
  - https://doc.rust-lang.org/rustdoc/index.html
---
> (méthodologie Bun 1.4, revue adversariale, toolchain rustc/cargo/rustdoc).


# Skill : Ingénierie Rust à l'Ère des Agents IA (2026)

Ce skill fournit à un agent IA (Claude Code, Codex, OpenCode, Cursor, etc.) les principes architecturaux, les workflows de migration de code à grande échelle, les commandes de la toolchain officielle Rust et les boucles de compilation ultra-rapides via `rustd`.

---

## 1. Quand choisir Rust (Matrice de décision)

Utilisez cette grille pour conseiller l'utilisateur ou choisir la stack d'un nouveau projet :

| Critère | Choisir **Rust** | Choisir **Python** | Choisir **Go / Zig** |
| :--- | :--- | :--- | :--- |
| **Outils CLI & Agents IA** | Démarrage instantané, binaire unique statique, faible empreinte mémoire (ex: Claude Code, OpenCode, ripgrep, serveurs MCP). | Prototypage rapide, scripts ponctuels. | Go si GC acceptable et simplicité réseau prioritaire. |
| **Infra IA & Inférence** | Tokenizers, moteurs d'inférence, orchestrateurs haute performance (xAI, Anthropic, Hugging Face, GitHub). | Entraînement ML, notebooks, écosystème PyTorch/Transformers. | Zig pour contrôle matériel bas niveau sans runtime caché. |
| **Sécurité mémoire & Concurrence** | Zéro *use-after-free*, zéro *double-free*, zéro *data race* garantis à la compilation via Ownership/Borrowing et RAII (`Drop`). | Géré par GC (au prix de la latence et du GIL/mémoire). | Zig exige une discipline stricte (`defer`/`errdefer`, guides de style). |
| **Développement assisté par IA** | **Boucle de rétroaction supérieure** : le compilateur (`rustc` + Borrow Checker) rejette immédiatement les hallucinations de gestion mémoire. | Erreurs découvertes tardivement au runtime. | Moins de garanties statiques sur les durées de vie (*lifetimes*). |

**Règle d'or :** Utilisez **Python** pour l'expérimentation ML/Data et la colle haut niveau ; utilisez **Rust** pour les runtimes, CLI, serveurs MCP, intégrations d'API IA critiques, parsers et tout système où la latence, la fiabilité et la distribution binaire comptent.

---

## 2. Principes Fondamentaux de Sécurité Mémoire (Pour Agents & Humains)

Lors de l'écriture ou de la revue de code Rust, imposez systématiquement ces garanties :

1. **Ownership & RAII (`Drop`)** : Chaque valeur possède un propriétaire unique. La libération des ressources (mémoire, sockets, descripteurs de fichiers) se fait automatiquement à la sortie de portée via le trait `Drop`, éliminant les fuites dans les chemins d'erreur (*error paths*).
2. **Borrowing** : Soit plusieurs références immuables (`&T`), soit une seule référence mutable (`&mut T`) à la fois — jamais les deux simultanément. Cela prévient les réentrances dangereuses (ex: mutation d'une `HashMap` pendant son itération ou détachement de buffer via callbacks).
3. **Le compilateur comme oracle** : Privilégiez toujours les erreurs de compilation aux conventions documentaires (*« Compiler errors are a better feedback loop than a style guide »*).
4. **Confinement de `unsafe`** : Si du code `unsafe` est indispensable (FFI C/C++, moteurs JS, allocateurs), isolez-le dans des abstractions sûres minimales, documentez les invariants avec `// SAFETY: ...`, et planifiez des passes de réduction de `unsafe` vérifiées par **Miri** et **ASAN**.

---

## 3. Méthodologie de Migration Massive assistée par IA (Playbook Bun 1.4)

Basé sur la réécriture de **Bun** (535 496 lignes de Zig → Rust en 11 jours, ~1M lignes de diff, 6 778 commits, jusqu'à 64 agents concurrents sur 4 *git worktrees* × 16 agents) :

### Phase 1 : Prérequis non négociables
- **Suite de tests indépendante du langage** : Ne modifiez *jamais* les tests existants pendant la migration. Les tests (ex: suite TypeScript de Bun avec >1M d'assertions) doivent servir d'oracle comportemental immuable.
- **Portage global vs incrémental** : Pour un runtime fortement couplé, privilégiez un **portage mécanique complet (all-at-once)** plutôt qu'une migration incrémentale générant du code glue temporaire difficile à supprimer.
- **Artefacts de spécification** : Avant de porter le code, générez :
  - `PORTING.md` : table de correspondance exacte des patterns, idiomes et types de la langue source vers Rust.
  - `LIFETIMES.tsv` : cartographie des durées de vie, allocateurs (arènes vs comptage de références) et contrats de libération.

### Phase 2 : Boucles dynamiques et Revue Adversariale
Ne demandez jamais à un agent unique « réécris tout sans erreur ». Orchestrez des workflows itératifs avec **fenêtres de contexte séparées (Split Context Windows)** :

1. **Agent Implémenteur** : Reçoit le fichier source original, `PORTING.md` et `LIFETIMES.tsv`. Produit le portage `.rs` fidèle à l'architecture existante.
2. **2+ Agents Reviewers Adversariaux** :
   - **Contexte strictement isolé** : ne voient *que* le diff (pas le raisonnement de l'implémenteur).
   - **Mandat unique** : partir du principe que le code est buggé et chercher exhaustivement les régressions, fuites, conditions de course, paniques et divergences sémantiques.
   - L'implémenteur ne fait pas de revue ; le reviewer n'écrit pas d'implémentation.
3. **Correction du processus, pas du diff** :
   - Lorsqu'un bug récurrent apparaît à l'échelle d'une flotte d'agents, **ne corrigez pas à la main** : mettez à jour le prompt, `PORTING.md` ou la boucle de workflow qui génère le code.

### Phase 3 : Jalons de convergence
1. Portage mécanique fichier par fichier (structure miroir maintenable par l'équipe).
2. Résolution des erreurs de compilation crate par crate (`cargo check`).
3. Tests de fumée (*smoke tests*) pour débloquer le runner interne (ex: `bun test`).
4. Boucle CI multi-plateformes (Linux, macOS, Windows) jusqu'au vert intégral sur tous les shards.
5. Passes post-migration : déduplication, nettoyage plateforme, réduction progressive des blocs `unsafe` vers du Rust idiomatique.

---

## 4. Boucle de Compilation Ultra-Rapide avec `rustd` (DioxusLabs)

Pour les humains et agents IA qui compilent le même workspace des centaines de fois (y compris en parallèle sur plusieurs *git worktrees*), utilisez le daemon **`rustd`** (jusqu'à 25× plus rapide sur les éditions incrémentales).

### Installation & Prérequis
`rustd` est épinglé à la toolchain **Rust 1.99.0** avec le composant `rust-src` (car il partage l'ABI et le layout de type via `rustc_abi`).

```bash
# 1. Installer la toolchain requise
rustup toolchain install 1.99.0 --component rust-src

# 2. Installer rustd (Linux x86-64/AArch64, macOS arm64) — backend Cranelift inclus
curl -fsSL https://dioxuslabs.github.io/rustd/install.sh | sh

# Alternative Windows PowerShell :
# irm https://dioxuslabs.github.io/rustd/install.ps1 | iex
```

### Workflow quotidien de l'agent avec `rustd`
```bash
# Premier build : cargo/rustc compile les dépendances externes, rustd charge le workspace
cargo rustd build

# Exécution rapide après édition d'une fonction (compilation fonction par fonction)
cargo rustd run -- --mon-flag

# Vérification complète du Borrow Checker et des diagnostics officiels rustc
cargo rustd check

# Gestion du daemon et du cache disque (~/.cache/rustd/store, max 8 GiB, partageable via S3)
cargo rustd status
cargo rustd stop
```

### Règles et garde-fous critiques pour `rustd`
- **Pas de Borrow Checker dans la boucle rapide** : `rustd` s'appuie sur `rust-analyzer` (parsing, typage, MIR) et Cranelift/LLVM. Un *use-after-move* ou un `mut` manquant peut compiler sous `cargo rustd build`. **Exécutez toujours `cargo rustd check` (qui invoque `cargo check` / `rustc`) avant tout commit ou validation finale.**
- **Pas de fallback silencieux** : Si une fonction ou une option (`--benches`, `lto = true`, `dylib`, `-Z`) n'est pas supportée, `rustd` échoue explicitement.
- **Oracle différentiel** : En cas de comportement inattendu du binaire généré par `rustd`, recompilez avec `rustc` standard (« *When they differ, rustd is wrong* »).
- **Mode édition uniquement** : Aucune inlining inter-fonctions ; utilisez `cargo build --release` pour la production.

---

## 5. Playbook de la Toolchain Officielle Rust

### Setup minimal recommandé (2026)
- **`rustup`** : gestion des toolchains (`rustup update`, `rustup component add clippy rustfmt rust-src`).
- **`cargo`** : gestionnaire de paquets, workspaces et build.
- **`rust-analyzer`** : serveur LSP (analyse sémantique, types inline, refactorings).
- **`clippy`** : linter idiomatique (`cargo clippy --all-targets -- -D warnings`).
- **`rustfmt`** : formatage automatique (`cargo fmt`).

### Commandes `cargo` essentielles
```bash
cargo new mon_projet          # Créer un binaire (--lib pour une bibliothèque)
cargo check                   # Vérification rapide des types et du borrow checker sans codegen
cargo build --release         # Build optimisé de production
cargo build --verbose         # Inspecter les invocations exactes de rustc
cargo test                    # Exécuter tests unitaires, d'intégration et doctests
cargo doc --open              # Générer et ouvrir la documentation HTML du crate et ses deps
```

### Invocation directe de `rustc`
- L'unité de traduction en Rust est le **crate**, pas le fichier individuel.
- On ne passe que la **racine du crate** (`main.rs` ou `lib.rs`) à `rustc` ; les déclarations `mod` résolvent le reste.
```bash
rustc src/main.rs             # Compile le crate binaire complet
rustc --crate-type lib src/lib.rs
```

### Documentation exécutable avec `rustdoc`
- **`///`** (*outer doc comment*) : documente l'élément qui suit (fonction, struct, trait, module).
- **`//!`** (*inner doc comment*) : documente l'élément englobant (utilisé en tête de `lib.rs` pour documenter le crate entier).
- **Doctests** : Tout bloc de code ` ```rust ` dans les commentaires de doc est compilé et exécuté automatiquement par `cargo test`.
- **Liens intra-doc** : Utilisez `[`MonType::ma_methode`]` pour créer des liens vérifiés par `rustdoc`.
- **Markdown autonome** : `rustdoc README.md` génère un HTML autonome.

---

## 6. Bonnes Pratiques d'Apprentissage & Projets Pratiques (Sans Dépendance Excessive à l'IA)

Si l'utilisateur apprend Rust en 2026 :
1. **Ne déléguez pas la compréhension** : Utilisez l'IA comme tuteur socratique (expliquer les messages d'erreur du borrow checker, comparer deux architectures) plutôt que pour générer du code aveuglément.
2. **Ressources de référence** : *The Rust Programming Language* (The Book), *Rustlings*, *The rustc Book*, *The rustdoc Book*, *The Cargo Book*.
3. **Projets recommandés pour monter en compétence** :
   - Client CLI performant interrogeant des API d'IA (streaming SSE, gestion d'erreurs typée avec `thiserror`/`anyhow`, async avec `tokio`).
   - **Serveur MCP (Model Context Protocol)** en Rust : binaire léger, démarrage rapide, typage fort des schémas JSON-RPC.
   - Outil CLI système (parser, indexeur de fichiers, wrapper d'automatisation).

---

## 7. Checklist de Validation Finale pour l'Agent

Avant de considérer une tâche Rust terminée :
- [ ] `cargo fmt --check` passe sans diff.
- [ ] `cargo check` (ou `cargo rustd check`) passe sans erreur de typage ni de borrow checker.
- [ ] `cargo clippy --all-targets` ne remonte aucun avertissement critique.
- [ ] `cargo test` (incluant les **doctests** `rustdoc`) passe à 100 %.
- [ ] Tout bloc `unsafe` introduit est strictement justifié par un commentaire `// SAFETY:` et minimisé.
- [ ] Les API publiques disposent de commentaires `///` (et `//!` à la racine du crate) vérifiés avec `cargo doc --no-deps`.
---

## Fit with this monorepo (checked 2026-11-21)

- **rustd:** DioxusLabs/rustd exists (last push 2026-11-03), and the crate `rustd` is at 0.1.0. Its README pins it to **Rust 1.99.0** with `rust-src`. fw's `rust-toolchain.toml` pins **1.100.0**, so rustd cannot drive this workspace until it follows 1.100.
  - Its README also says `cargo rustd` builds programs on Linux (ELF) and macOS arm64 only, so use it on the Linux cluster nodes, not for Windows targets.
  - Keep `cargo rustd check` (real rustc and the borrow checker) as the gate before every commit.
- **Pure-Rust port train:** `docs/research/pure-rust-audit-2026-11-21.md` applies section 3 of this skill. Each port batch produces `PORTING.md` and `LIFETIMES.tsv`, freezes the existing tests as the oracle, and gets an implementer agent plus adversarial reviewers that see only the diff.
