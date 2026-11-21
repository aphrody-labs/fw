---
name: gemini-api
description: "Gemini API and SDK development: models, generation, function calling, Live API, Omni Flash, and the gemini CLI. Use when writing code against Gemini."
---
# Gemini API

Read the reference that matches the Gemini surface in use.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/gemini-api-dev.md](references/gemini-api-dev.md) | Use when writing code that calls the Gemini API for text generation, multi-turn chat, multimodal understanding, image generation, video generation, speech generation (TTS), voice design |
| [references/gemini-live-api-dev.md](references/gemini-live-api-dev.md) | Use when building real-time, bidirectional streaming applications with the Gemini Live API, or migrating legacy Live models (2.0/2.5/3.1) to Gemini 3.8 Live. Covers WebSocket-based audio/video/text |
| [references/gemini-omni-flash-api.md](references/gemini-omni-flash-api.md) | Use for generative video editing, text-to-video, image- and video-referenced video generation, first-frame-to-video, first-and-last-frame transitions |
| [references/gemini-api-cli.md](references/gemini-api-cli.md) | Call the Gemini API from the shell with gemini-api. Use for media generation (images, TTS, music, video), file and YouTube analysis, audio/video transcription, Deep Research, and sandboxed agents. |

<!-- merge-skill-families:v1 -->
