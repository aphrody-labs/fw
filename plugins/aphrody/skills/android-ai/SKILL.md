---
name: android-ai
description: "On-device AI on Android: LiteRT-LM and LiteRT runtime, ML Kit GenAI prompt and agent APIs, AppFunctions."
---
# Android on-device AI

Pick the reference for the on-device model or API in use.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

| Reference | Use when |
| --- | --- |
| [references/litert-lm.md](references/litert-lm.md) | Builds an Android app that runs an open text LLM on the device, on the CPU or the GPU. Use when creating a chat, summarization, or extraction feature with a.litertlm model from Hugging Face |
| [references/litert-runtime.md](references/litert-runtime.md) | Configures and implements LiteRT (.tflite) on-device inference on Android in Kotlin using the CompiledModel API on CPU and GPU. Use for any Android LiteRT or.tflite task, including Gradle setup |
| [references/ml-kit-genai-prompt-api.md](references/ml-kit-genai-prompt-api.md) | Analyzes Android codebases to implement ML Kit GenAI Prompt API. Use to send natural language requests on-device to Gemini Nano, use structured output with Prompt API, implement prefix caching |
| [references/mlkit-agent-api.md](references/mlkit-agent-api.md) | Builds autonomous on-device agentic workflows in Android apps using the ML Kit GenAI Agent API powered. Use when implementing conversational agents, multi-turn chat with AgentSession in a ViewModel |
| [references/appfunctions.md](references/appfunctions.md) | Analyzes Android apps to find user workflows to expose as AppFunctions, such as creating a note. Use when discovering, implementing, documenting, testing, or upgrading AppFunctions in an Android app |

<!-- merge-skill-families:v1 -->
