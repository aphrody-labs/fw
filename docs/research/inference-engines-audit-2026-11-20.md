# Audit moteurs d'inférence : vLLM, llama.cpp/ggml, llm-d et équivalents Rust (2026-11-20)

Sources : `gh api` (métadonnées, arbres, releases, docs) et crates.io, relevés le 2026-11-20. Chiffres re-vérifiés en direct : versions et étoiles ci-dessous. Aucun benchmark exécuté, aucun code amont lu en profondeur.
Complète `docs/GGML_COMPATIBILITY.md` (scan ggml-org du 2026-11-19) et `rust-competitors-matrix-2026-11-19.md` sans les répéter. Le snapshot vivant est `docs/research/upstream-watch.tsv` (cron toutes les 6 h).

## 1. Amont

| Projet | Étoiles | Langage | Release | Remarque |
|---|---|---|---|---|
| vllm-project/vllm | 97265 | Python 85,9 %, Rust 6,7 % | v0.34.0 (2026-11-16) | un minor tous les 13 à 15 jours, 10770 PR+issues ouvertes |
| ggml-org/llama.cpp | 135837 | C++ | v1.0.0 (2026-11-13) | environ 16 tags `bNNNN` par jour (493 depuis 2026-10-21) |
| ggml-org/ggml | 16130 | C++ | v1.0.0 (2026-11-13) | synchronisé depuis llama.cpp, développement réel côté llama.cpp |
| llm-d/llm-d | 5320 | Python (guides) | v1.0.0 (2026-10-20) | CNCF Sandbox, 0 ligne de Rust en production |
| llm-d/llm-d-router | 473 | Go | v1.0.0 (2026-10-16) | remplace `llm-d-inference-scheduler` |
| kubernetes-sigs/gateway-api-inference-extension | 806 | Go | v1.6.2 (2026-09-17) | API `InferencePool` ; l'EPP migre vers llm-d-router |

### vLLM
- Moteur V1 : Model Runner V1 supprimé en v0.34.0 (MRV2 seul), `--block-size` 64 par défaut, `gpu_memory_utilization` 1.0 par défaut.
- Scheduler et KV : `vllm/v1/core/sched/`, `block_pool.py`, `kv_cache_manager.py`, prefix caching automatique, connecteurs KV (nixl, mooncake, lmcache, moriio, hf3fs, offloading) et offload `cpu`/`fs`/`tiering`.
- Spéculatif : eagle, mtp, ngram, suffix, dflash, draft model.
- Matériel : CUDA, ROCm, XPU, CPU (x86, ARM64, s390x), Apple via `vllm-metal` (MLX). Pas de Windows natif (WSL ou fork communautaire).
- API : OpenAI, Anthropic, Cohere, gRPC, MCP ; structured outputs xgrammar, guidance, outlines.
- **Rust dans vLLM** : `vllm/rust/` est un workspace de 16 crates (424 fichiers `.rs`, toolchain 1.95), frontend HTTP/gRPC (axum, tonic) qui parle au moteur Python en ZMQ + MessagePack. Activation `VLLM_USE_RUST_FRONTEND=1`. Suivi : issues #68784 (défaut) et #44280 (parité). Satellites Rust : `vllm-project/router` (v0.1.17), `agentic-api` (v0.10.0).
- Écosystème : `llm-compressor`, `speculators`, `guidellm`, `aibrix` (Go), `semantic-router` (Go), `production-stack`, plugins matériels (Ascend, TPU, Gaudi, Neuron, XPU, Metal).

### llama.cpp / ggml
- ggml : tenseurs 4D (`ne`/`nb`), `ggml_cgraph`, scheduler de backends ; 18 backends (blas, cann, cpu, cuda, et, hexagon, hip, metal, musa, opencl, openvino, rpc, sycl, virtgpu, vulkan, webgpu, zdnn, zendnn).
- Quantifications de `ggml_type` : classiques, K-quants Q2_K à Q8_K, I-quants (IQ1 à IQ4), ternaires TQ1_0/TQ2_0, récents MXFP4, NVFP4, Q1_0, Q2_0, F8_E4M3. GGUF reste en version 3.
- llama-server : OpenAI, Anthropic `/v1/messages`, rerank, embeddings, batching continu, spéculatif, tool calling, MCP, schéma JSON, mtmd, slots save/restore, LoRA, Prometheus, mode router multi-modèles (`--models-dir`).
- v1.0.0 : retrait de 35 fonctions dépréciées de `llama.h`, cache MoE en mémoire hôte, KV cache FP8, chargement non bufferisé sous Windows.
- Rust : `utilityai/llama-cpp-rs` (crates `llama-cpp-2` 0.1.161, 941k téléchargements récents) est le seul binding actif ; il épingle un commit de llama.cpp.

### llm-d
- Routeur (EPP) appelé par Envoy en ext-proc : pipeline Filter-Score-Pick, scorers préfixe-cache et charge en tokens, flow control, routage par latence prédite (expérimental).
- Désagrégation P/D par `pd-sidecar` et transfert KV via NIXL (RDMA requis pour être efficace, TCP uniquement pour tests). Wide-EP (DeepEP, MoRI-EP), offload KV par paliers, KEDA, batch-gateway, async.
- Moteurs : vLLM (défaut), SGLang, TensorRT-LLM.
- Mode sans Kubernetes : EPP + Envoy + vLLM en processus, `endpoints.yaml` rechargé à chaud (pas de flow control piloté par `InferenceObjective`).
- Risque : réorganisation récente, anciens noms redirigés ou archivés (inference-scheduler, kv-cache-manager, routing-sidecar, WVA).

## 2. Équivalents Rust (crates.io, `recent_downloads` sur environ 90 jours)

| Besoin | Meilleur Rust | État |
|---|---|---|
| Serveur type vLLM léger | `EricLBuehler/mistral.rs` v0.9.5, 7851 étoiles | continuous batching, PagedAttention CUDA/Metal, prefix cache, ISQ/GGUF/GPTQ/AWQ/FP8, API OpenAI et Anthropic ; crate `mistralrs` 0.8.1, 83k, en retard sur GitHub. ROCm et Vulkan non trouvés |
| Idem, variantes | `candle-vllm` (742), `xinfer` (356), `pegainfer` (748, CUDA seul) | spéculatif et P/D annoncés dans les README, non vérifiés ; pas de crate publiée |
| Tenseurs, kernels | `huggingface/candle` (21359, `candle-core` 0.11.1, 5,06M), `tracel-ai/burn` (16315), `tracel-ai/cubecl` (2510, 0.11.4, 1,11M) | cubecl couvre CUDA, ROCm, Metal, Vulkan, WebGPU, CPU : meilleure base ggml-like portable en Rust |
| Serveur GGUF WebGPU | `shimmy` (6021) | API OpenAI partielle |
| Binding llama.cpp | `llama-cpp-2` | stable, actif |
| Routeur / passerelle | `smg-project/smg` v1.15.0 (734), `vllm-project/router` | routage cache-aware (radix trees), P/D, multi-moteurs, API OpenAI et Anthropic |
| Orchestration | `ai-dynamo/dynamo` v1.6.2 (8539) | cœur Rust 31,6 Mo (`dynamo-kv-router`, runtime) au-dessus de moteurs Python |
| Mesh | `Mesh-LLM/mesh-llm` v0.80.0 (4179) | hôte Rust autour d'un llama.cpp patché ; déjà suivi |
| Formats | `safetensors` 0.8.0 (15,6M), `tokenizers` 1.0.0-rc.5 (20,9M) | références, communes à tout l'écosystème |
| TGI | `huggingface/text-generation-inference` | archivé (2026-03), routeur Rust, inférence Python |

### Sans équivalent Rust crédible
- Remplaçant de ggml : aucun Rust ne combine kernels quantifiés K/I-quants, tous backends et débit comparable. Les crates `ggml` (2023), `gguf`, `ggus`, `gguf-rs` sont des parseurs ou prototypes.
- Parité vLLM sur la longue traîne de modèles, LoRA multi-tenant, quantifs MoE.
- Planificateur llm-d complet (indexation globale du cache KV, sélection de pods) : seul `dynamo-kv-router` s'en approche côté Rust.
- Désagrégation P/D de bout en bout dans un moteur Rust : annoncée par pegainfer et xinfer, non vérifiée.

## 3. Écart avec fw (état du code lu le 2026-11-20)

| Fonction | Amont | fw | Écart |
|---|---|---|---|
| Types GGML | ~40 types (K, I, MXFP4, NVFP4, TQ, Q1_0, F8) | `GgmlType` : 36 ids vivants de `enum ggml_type` (ggml-org/ggml master), tailles de bloc testées contre `ggml-common.h` | fait le 2026-11-20 |
| GGUF | v3, sidecars `mmproj` et `mtp`, big-endian | v2/v3 : en-tête, métadonnées, table des tenseurs (tous les types), encodeur ; `GgufSidecar` (mmproj, mtp, dflash, eagle3, dspark) | big-endian non lu |
| Backends | 18 | `fw-llama` délègue à llama.cpp ; `fw-ggml` définit le graphe et l'enum de backends | exécution native des kernels absente, pas de reproduction de ggml |
| Serveur | continuous batching, router multi-modèles | `fw-llama/server` | écart à mesurer sur le mode router et les slots |
| Routage cluster | cache-aware radix (smg, vllm-router, llm-d EPP), P/D | `fw-cluster::epp` : profils Filter-Score-Pick, scorers préfixe-cache (index de hachages de blocs, `observe` et `KvEvent`), charge en tokens, profondeur de file, capacité ; profil P/D prefill puis decode | pas de pd-sidecar ni de transfert KV réel ; index approximatif sur octets de prompt, pas de tokenizer |
| Spéculatif | eagle, mtp, ngram, dflash | `prefer_user_webgpu_draft` dans le routeur | pas de pipeline spéculatif propre |
| Frontend Rust de vLLM | workspace `vllm/rust/` (tokenizer, parsers, chat) | non couplé | source de crates à réutiliser (parsers d'outils et de raisonnement) |

## 4. Décisions
1. **Ne pas réécrire ggml en Rust.** Garder `fw-ggml` comme couche format/graphe/RPC et `fw-llama` (llama.cpp) pour l'exécution ; assembler plutôt qu'écrire. Version llama.cpp épinglée côté `fw-llama-sys` : en-têtes vendus alignés sur v1.0.0 (`include/LLAMA_CPP_TAG`, `bun sync-headers.ts`) ; v1.0.0 a retiré 35 fonctions.
2. **Étendre `GgmlType`** aux types récents (IQ*, TQ*, MXFP4, NVFP4, Q1_0, Q2_0, F8_E4M3) en lisant `ggml.h` du dépôt `ggml-org/ggml` (référence), avec tests d'octets par bloc.
3. **Routage cluster** : faire évoluer `fw-cluster` vers le modèle EPP de llm-d (Filter, Score, Pick avec scorers préfixe-cache et charge), en s'inspirant de `smg` et `dynamo-kv-router` ; le mode sans Kubernetes de llm-d est la cible logique pour un parc de quelques hôtes.
4. **Moteur Rust natif** : si un moteur propre est voulu, partir de `candle` ou `cubecl` + `mistral.rs` (assembler), pas de zéro.
5. **Sources à réutiliser** : `vllm/rust/` (parsers tool/reasoning, tokenizer), `smg` (routage), `safetensors`, `tokenizers`.

## 5. Suivi automatique
Ajoutés à `upstreams.toml` avec `watch = true` : ggml, vllm, vllm-router, llm-d, llm-d-router, gateway-api-inference-extension, dynamo, smg, mistral-rs, candle, cubecl, llama-cpp-rs ; `llama-cpp` est passé en `watch = true`. `mesh-llm` était déjà suivi.

## Non vérifié
- Aucun benchmark ni build des projets cités ; débits des README non reproduits.
- Fonctions annoncées par README seulement : spéculatif de candle-vllm, xinfer, pegainfer ; backends détaillés de mesh-llm ; Metal de candle (lu dans `Cargo.toml`).
- Lien entre `smg` et `sgl-model-gateway`, et état du `sgl-router` de SGLang.
- Comptes d'issues par mot-clé (bruités) non repris ici.
