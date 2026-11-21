# Upstreams de harness d'agents: CLI-Anything, ECC, claude-mem, mesh-llm, OpenDuet, RTK, Graphify, Agent Skills

Lecture live du 2026-11-20 par `gh api` (arbres, README, releases, commits, issues). Aucun dépôt cloné. Les valeurs qui dérivent (étoiles, release, HEAD) vivent dans `docs/research/upstream-watch.tsv`, rafraîchi toutes les 6 h par `.github/workflows/sync-upstreams.yml` (`just sync-research` en local). Les trois dépôts sont déclarés dans `upstreams.toml` avec `watch = true`.

## Vue d'ensemble

| | CLI-Anything | ECC | claude-mem |
|---|---|---|---|
| Dépôt | `HKUDS/CLI-Anything` | `affaan-m/ECC` | `thedotmack/claude-mem` |
| Licence | Apache-2.0 | MIT (sous-arbres avec LICENSE propre) | Apache-2.0 (zones commerciales réservées) |
| Dernière release lue | v0.4.0, 2026-06-25 | v2.2.3, 2026-10-01 | v16.0.1, 2026-11-19 |
| Dernier commit lu | main: 2026-09-22 (code: 2026-08-21) | 2026-11-20 | 2026-11-19 |
| PR + issues ouvertes | 174 + 102 | 47 + 90 | 25 + 60 |
| Langage | Python | JavaScript, un scaffold Rust alpha (`ecc2/`) | TypeScript (Bun) |
| Rythme | gelé sur main, activité dans les PR | quotidien | plusieurs majeures par semaine |

## CLI-Anything

- Génère des CLI Python (Click, REPL, `--json`) autour d'un logiciel réel via un SOP en 7 phases (`cli-anything-plugin/HARNESS.md`): analyse, conception, implémentation, plan de tests, tests, documentation, publication. Le logiciel réel est une dépendance dure, sans repli.
- `cli-anything-plugin/skill_generator.py` produit `SKILL.md` en parsant les décorateurs Click avec `ast` (sans exécuter le code), à partir de `templates/SKILL.md.template`. `.github/scripts/validate_root_skills.py` et `sync_root_skills.py` gardent la copie du skill alignée sur le canonique.
- Registre: `registry.json` (79 entrées), `public_registry.json` (24), `matrix_registry.json`. Champs: `install_cmd`, `entry_point`, `skill_md`, `package_manager`. Gestionnaire `cli-hub/` (`list/search/info/install/update/uninstall/launch`).
- Carte A2A dans `docs/hub/.well-known/agent-card.json`. Aucun fichier ACP.
- Risques: installation depuis `main` sans verrou de commit ni checksum (`pip install git+...`), `shell=True` dans `installer.py` pour les commandes à métacaractères, verrou de session ignoré sous Windows en cas d'`OSError`, lien `openclaw-skill` mort, « 2,464 passed » non reproduit.
- À reprendre dans `fw`: le générateur `SKILL.md` par `ast` (porté sur les attributs clap), le schéma de registre, le garde de dérive des skills, la carte A2A pour `fw-a2a`. À écarter: le hub tel quel.

## ECC

- Plugin Claude Code (`.claude-plugin/plugin.json`, marketplace `ecc`): 328 skills, 72 agents, 99 commandes, 122 règles en 25 packs, 28 entrées de hooks (comptés dans l'arbre). Adaptateurs pour Codex, Cursor, OpenCode, Gemini, Zed et d'autres.
- Hooks à profils: `scripts/hooks/run-with-flags.js` + `ECC_HOOK_PROFILE` + `ECC_DISABLED_HOOKS`, schéma `schemas/hooks.schema.json`, validation `scripts/ci/validate-hooks.js`, `hook-watchdog.js`.
- Instincts (`skills/continuous-learning-v2/`): hooks Pre/PostToolUse, observateur Haiku, instincts atomiques avec confiance 0.3-0.9, portée projet puis promotion globale quand l'instinct apparaît dans 2 projets. Proche de la boucle d'apprentissage du skill `learn`.
- Mémoire portable `ecc.memory.v1` (Memory Vault Markdown, `scripts/memory.js`, `memory-mcp.mjs`), exige `npm i -g ecc-universal`.
- Sécurité: AgentShield (`ecc-agentshield`), `gateguard-fact-force.js` (refuse la première édition tant que les faits ne sont pas cités), scans supply-chain et unicode en CI.
- Installateur déclaratif: `manifests/install-modules.json` (49 modules), `install-profiles.json`, `scripts/doctor.js`, `repair.js`.
- Risques: `.codex-plugin/plugin.json` annonce 281 skills contre 328 ailleurs, duplication par harness, hooks lancés par `node -e` avec résolveur de racine inline, le CLAUDE.md d'ECC impose 80 % de couverture et du TDD, ce qui heurte la doctrine « zéro garde bloquante ». Le code des hooks n'a pas été audité.
- À reprendre dans `fw`: le modèle de données des instincts, le gating des hooks par profil, le format de manifeste d'installation, le garde de faits avant édition (sans blocage dur).

## claude-mem

- Hooks Claude Code: SessionStart, UserPromptSubmit, PostToolUse(+Failure), PreToolUse (`file-context`), Stop, SessionEnd. Chaque hook exécute `claude-mem hook claude-code <event>` sans shell, budget de remise 250 ms, abandon silencieux.
- Worker HTTP local (Bun) sur `127.0.0.1`, port 37700 + offset par installation (`src/shared/install-port.ts`), garde anti DNS-rebinding, token API local.
- Stockage `~/.claude-mem/claude-mem.db` (SQLite WAL, FTS5 à contenu externe avec triggers); Chroma et un embedder onnxruntime optionnels.
- Un « observer » (second agent LLM, défaut `haiku` sur abonnement Claude) transforme chaque tool call en XML `<observation>` (types: bugfix, feature, refactor, change, discovery, decision, security_alert...) puis un `<summary>` au Stop. Dédup tf-idf (`src/services/dedup/`), classement ACT-R optionnel.
- Recherche MCP à 3 niveaux: `search` (index ~50-100 tokens par résultat), `timeline`, `get_observations`; injection au SessionStart bornée par `ContextBudget.ts`.
- Confidentialité: balises `<private>` etc. retirées avant stockage (`src/utils/tag-stripping.ts`, passe linéaire, gère l'imbrication).
- Risques: télémétrie PostHog active par défaut (coupée par `DO_NOT_TRACK`), l'installateur interactif propose un provider cloud `cmem`, l'observer envoie les tool calls au modèle choisi, worker permanent, API instable (v15.21.0, v16.0.0, v16.0.1 en 3 jours).
- Écart avec `fw`: `fw mem` (3 couches BM25) et `learn.ts` (FTS5, `bm25(chunks,0,0,4,1)`) n'ont ni capture par hook, ni résumé LLM, ni injection automatique au démarrage de session. C'est l'écart réel.
- À reprendre dans `fw`: hooks sans shell avec budget 250 ms, strip des balises de confidentialité, schéma d'observation et types, budget de contexte à 3 niveaux, port par installation. À écarter: Chroma, onnxruntime, uvx.

## Ordre d'intégration proposé

1. `claude-mem`: capture par hook + observer local (OpenAI-compatible, ollama ou llama.cpp via `fw-ggml`) dans `fw mem`, FTS5 seul.
2. `ecc`: profils de hooks et schéma d'instincts dans `plugin/rules` et `learn.ts`.
3. `CLI-Anything`: générateur `SKILL.md` + garde de dérive dans `xtask`.

## Veille 2026-11-14 (vidéo YouTube EC9rRce4DRQ), vérifiée le 2026-11-20

- **mesh-llm** (`Mesh-LLM/mesh-llm`, Rust, Apache-2.0, v0.80.0 du 2026-11-16): le README annonce une API compatible OpenAI sur `http://localhost:9337/v1` et des découpes par étages du moteur Skippy pour les modèles trop gros pour une machine. Ajouté à `upstreams.toml` (cible `fw-cluster` + `fw-ggml`).
- **OpenDuet** (`CopilotKit/OpenDuet`, TypeScript, MIT): routines planifiées, mémoire, approbation avant action, auto-hébergé. Le README exige un projet CopilotKit Intelligence pour les threads et la mémoire, donc pas 100 % local. Aucune release publiée. Ajouté à `upstreams.toml`.
- **Nanbeige4.3-33B** (`Nanbeige/Nanbeige4.3-33B` sur Hugging Face): Apache-2.0, 33,1 G de paramètres, dernière modification 2026-10-28, quantifications GGUF tierces disponibles. La fenêtre de 512K tokens et les scores comparés à Qwen3.6-27B et Gemma 4 viennent de la vidéo et ne sont pas confirmés par la fiche lue.
- **Non vérifiés** (aucun dépôt ou jeu de données trouvé par recherche GitHub/Hugging Face): Hallmark 2.0, Opera WebAI, les ~2 575 modèles 3D physiques NVIDIA sur Hugging Face.

## Veille plugins agents 2026-11-16 (vidéo YouTube 6ebGpDRjmbE), vérifiée le 2026-11-20

- **RTK** (`rtk-ai/rtk`, Rust, Apache-2.0, branche par défaut `develop`): la dernière release lue est v1.2.1 du 2026-11-19, pas la v0.34.3 de la vidéo. Le README annonce 60-90 % de tokens en moins sur les commandes de dev courantes. Il s'installe par un hook qui réécrit les commandes shell (`rtk init -g` pour Claude Code) ; `Read`, `Grep` et `Glob` passent sans compression par défaut. Windows par winget. Complément direct de `fw sh`.
- **Graphify** (`Graphify-Labs/graphify`, ex `safishamsi/graphify`, Python, Apache-2.0, branche `v8`): v0.9.113 du 2026-11-19. Graphe interrogeable du code, des docs, des schémas SQL, des configs et des PDF. `fw graph` en reprend déjà l'idée sans dépendance.
- **Agent Skills** (`addyosmani/agent-skills`, MIT, branche `main`): v0.6.17 du 2026-11-18, 26 dossiers dans `skills/` (la vidéo en annonce 25). À comparer avec `plugin/skills` avant tout import pour éviter les doublons.
- Les trois sont dans `upstreams.toml` avec `watch = true`, donc suivis par le cron.
