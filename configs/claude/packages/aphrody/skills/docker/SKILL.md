---
name: docker
description: "Docker work: Dockerfiles and image size or build speed, Compose stacks, containerizing a project, Docker Agent (agent.yaml, run, deploy), Docker Sandboxes (sbx lifecycle, kits, network and credentials). Read the destructive-guardrails reference BEFORE any docker command that deletes, prunes, resets or cleans state. Use docker-docs for documentation questions."
---
# Docker (except documentation lookup)

Pick the reference that matches the task and read it in full before acting. Documentation questions about any Docker product go to the separate `docker-docs` skill.

Read the reference that matches the task in full before acting. Paths written inside reference files are relative to this skill's directory (the one holding this SKILL.md). Where a reference names another merged skill, open `references/<name>.md`; its helper files are under `references/<name>/`.

- Safety first: before running or recommending any docker command that deletes, wipes, prunes, resets or irreversibly changes state (even for a request like "clean up" or "start fresh"), read `references/docker-destructive-guardrails.md`.

| Reference | Use when |
| --- | --- |
| [references/core.md](references/core.md) | Former body of this skill (read first) |
| [references/docker-destructive-guardrails.md](references/docker-destructive-guardrails.md) | Use this skill before running, or recommending, any Docker command that deletes, wipes, resets, or otherwise irreversibly changes state — even if the user just says to "clean up", "clear the cache", "start fresh", "wipe everything", "nuke it", "reset", "force remove", or "tear down" Docker resources. Covers generic... |
| [references/docker-project-foundations.md](references/docker-project-foundations.md) | Use this skill when setting up, initializing, or Dockerizing a project, even if the user doesn't explicitly mention Docker but describes a need for containerized local development, adding a database or cache dependency, or running services without host-level installs. Covers Dockerfile, compose.yaml, and .dockerignore... |
| [references/docker-build-strategies.md](references/docker-build-strategies.md) | Use this skill when writing, reviewing, or optimizing Dockerfiles, even if the user just says their image is too large, their build is slow, or they need to harden a container for production. Covers multi-stage builds, layer caching, .dockerignore, non-root users, and image size optimization. |
| [references/docker-compose-patterns.md](references/docker-compose-patterns.md) | Use this skill when creating, modifying, or debugging Docker Compose configurations, even if the user just says they need to wire services together, add a database to their stack, or set up a local development environment with multiple containers. Covers service definitions, health checks, dependency ordering,... |
| [references/docker-agent-config.md](references/docker-agent-config.md) | Use this skill when creating or editing an agent.yaml (or .yml/.hcl) configuration file for Docker Agent (cagent), including defining agents, models/providers, built-in or MCP toolsets, multi-agent teams with sub_agents. Even if the user just says they want to "build an AI agent with Docker", "make a coding agent... |
| [references/docker-agent-run.md](references/docker-agent-run.md) | Use this skill when running a Docker Agent with `docker agent run`, choosing a safety/approval mode, using the `--sandbox` isolation flag, setting up aliases, or troubleshooting a run (missing credentials, worktrees). Even if the user just says they want to "run my agent", "make my agent auto-approve tool calls", "run... |
| [references/docker-agent-deploy.md](references/docker-agent-deploy.md) | Use this skill when exposing a Docker Agent as a server (MCP, HTTP API, A2A, ACP, or OpenAI-compatible chat), distributing an agent via an OCI registry with `docker agent share`, or measuring agent quality with `docker agent eval`. Even if the user just says they want to "turn my agent into an MCP server", "let Claude... |
| [references/docker-sandboxes-lifecycle.md](references/docker-sandboxes-lifecycle.md) | Use this skill when creating, running, reattaching to, listing, stopping, or removing Docker Sandboxes (the standalone `sbx` CLI that runs AI coding agents in isolated microVMs), even if the user just says they want to "run claude in a sandbox", "isolate an agent from my repo", "give an agent its own git clone", or... |
| [references/docker-sandboxes-network-credentials.md](references/docker-sandboxes-network-credentials.md) | Use this skill when configuring what a Docker Sandboxes (`sbx`) sandbox can reach on the network or which credentials it authenticates with, even if the user just says they want to "let the agent call an internal API", "block all network access", "give the agent a GitHub token", or "use a private registry image for a... |
| [references/docker-sandboxes-kits.md](references/docker-sandboxes-kits.md) | Use this skill when authoring, verifying, publishing, or composing a Docker Sandboxes kit, even if the user just says they want to "add a tool to a sandbox agent", "package a CLI or agent as a kit", "publish a kit to a registry", or "give a mixin its own credentials and network access". Covers the v3 kit descriptor... |

<!-- merge-skill-families:v1 -->
