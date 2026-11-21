---
name: huggingface
description: "Hugging Face Hub and ML work: hf CLI, models, datasets, Spaces, Gradio, ZeroGPU, papers, training (TRL, sentence-transformers, vision, Unsloth), evals, local GGUF models, Transformers.js, Trackio."
---
# Hugging Face

Pick the reference for the Hub task. Python tooling runs through `uv` (`uv run`, `uv pip install`), never pip.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/hf-cli.md](references/hf-cli.md) | Hugging Face Hub CLI (hf) for downloading, uploading, and managing models, datasets, spaces, buckets, repos. Use when: handling authentication; managing local cache; managing Hugging Face Buckets |
| [references/hf-mem.md](references/hf-mem.md) | Estimate the GPU memory needed to run a Hugging Face model (weights. Use when asked how much VRAM a model needs, whether it fits on a GPU, the longest context that fits |
| [references/huggingface-best.md](references/huggingface-best.md) | Use when the user asks about finding the best, top, or recommended model for a task. Triggers on: "best model for X", "what model should I use for", "top models for [task]" |
| [references/huggingface-community-evals.md](references/huggingface-community-evals.md) | Run evaluations for Hugging Face Hub models using inspect-ai and lighteval on local. Use for backend selection, local GPU evals, and choosing between vLLM / Transformers / accelerate. Not for HF |
| [references/huggingface-datasets.md](references/huggingface-datasets.md) | Use for Hugging Face Dataset Viewer API workflows that fetch subset/split metadata, paginate rows, search text, apply filters, download parquet URLs, and read size or statistics. |
| [references/huggingface-gradio.md](references/huggingface-gradio.md) | Build Gradio web UIs and demos in Python. Use when creating or editing Gradio apps, components, event listeners, layouts, or chatbots. |
| [references/huggingface-llm-trainer.md](references/huggingface-llm-trainer.md) | Train or fine-tune language and vision models using TRL (Transformer Reinforcement Learning) or Unsloth with Hugging Face Jobs. Use for tasks involving cloud GPU training, GGUF conversion |
| [references/huggingface-local-models.md](references/huggingface-local-models.md) | Use to select models to run locally with llama.cpp and GGUF on CPU, Mac Metal, CUDA, or ROCm. Covers finding GGUFs, quant selection, running servers, exact GGUF file lookup, conversion |
| [references/huggingface-lora-space-builder.md](references/huggingface-lora-space-builder.md) | Build and publish a Gradio demo on Hugging Face Spaces for a user-provided LoRA. Use when someone asks to create, generate, ship, or publish a Space, demo, Gradio app |
| [references/huggingface-paper-publisher.md](references/huggingface-paper-publisher.md) | Publish and manage research papers on Hugging Face Hub. Supports creating paper pages, linking papers to models/datasets, claiming authorship |
| [references/huggingface-papers.md](references/huggingface-papers.md) | Look up and read Hugging Face paper pages in markdown. Use when the user shares a Hugging Face paper page URL, an arXiv URL or ID, or asks to summarize, explain |
| [references/huggingface-spaces.md](references/huggingface-spaces.md) | Build, deploy, and maintain applications on Hugging Face Spaces — Gradio / Docker / Static SDKs. Use whenever the user asks to create or host an app on Hugging Face, port code onto ZeroGPU |
| [references/huggingface-tool-builder.md](references/huggingface-tool-builder.md) | Use when the user wants to build tool/scripts or achieve a task where using data from the Hugging Face API would help. This is especially useful when chaining or combining API calls or the task will |
| [references/huggingface-trackio.md](references/huggingface-trackio.md) | Track and visualize ML training experiments with Trackio. Use when logging metrics during training (Python API), firing alerts for training diagnostics |
| [references/huggingface-vision-trainer.md](references/huggingface-vision-trainer.md) | Trains and fine-tunes vision models for object detection (D-FINE, RT-DETR v2, DETR, YOLOS). Use when users mention training object detection, image classification, SAM, SAM2, segmentation |
| [references/huggingface-zerogpu.md](references/huggingface-zerogpu.md) | AI demos and GPU compute with Gradio Spaces and Hugging Face Spaces ZeroGPU. Use when writing or reviewing code that uses @spaces.GPU, configuring python_version or requirements.txt for a ZeroGPU |
| [references/train-sentence-transformers.md](references/train-sentence-transformers.md) | Train or fine-tune sentence-transformers models across SentenceTransformer (bi-encoder, dense or static embedding model for retrieval, similarity. Use for any sentence-transformers training task. |
| [references/transformers-js.md](references/transformers-js.md) | Use Transformers.js to run state-of-the-art machine learning models directly in JavaScript/TypeScript. Supports NLP (text classification, translation, summarization) |
| [references/trl-training.md](references/trl-training.md) | Post-train LLMs with TRL (Transformers Reinforcement Learning) — SFT, DPO, GRPO, KTO, and reward-model training. Use when writing or debugging training code with the TRL Python API or the trl CLI. |

<!-- merge-skill-families:v1 -->
