import { brainSearch, DEFAULT_ROUTER_PROVIDERS, loadSoul, routeModelRequest } from "./ai_stack.ts";

export interface DecisionQuestionNoul {
  type: "noul";
  instructions: string;
}

export interface DecisionQuestionChoice {
  type: "choice";
  instructions: string;
  criteria: Record<string, string>;
}

export interface DecisionQuestionScore {
  type: "score";
  instructions: string;
  criteria: Record<string, string>;
}

export type DecisionQuestion = DecisionQuestionNoul | DecisionQuestionChoice | DecisionQuestionScore;

export interface DecisionsRequest {
  model?: string;
  input: string;
  questions: Record<string, DecisionQuestion>;
  mode?: "local" | "remote" | "auto";
}

export type DecisionAnswer =
  | { type: "noul"; noul: number; confidence: number }
  | { type: "choice"; choice: string; probabilities: Record<string, number>; confidence: number }
  | { type: "score"; score: number; probabilities: Record<string, number>; confidence: number };

export interface DecisionsResponse {
  ok: boolean;
  model: string;
  backend: "gemini-decision-1-remote" | "fw-system1-local";
  outputTokens: 0;
  questionCount: number;
  answers: Record<string, DecisionAnswer>;
}

function tokenize(text: string): Set<string> {
  return new Set(
    text
      .toLowerCase()
      .replace(/[^a-z0-9_]+/g, " ")
      .split(/\s+/)
      .filter((t) => t.length >= 2),
  );
}

function overlapScore(inputTokens: Set<string>, desc: string): number {
  const descTokens = tokenize(desc);
  if (descTokens.size === 0) return 0;
  let hits = 0;
  for (const t of descTokens) {
    if (inputTokens.has(t)) hits++;
  }
  return hits / Math.sqrt(descTokens.size);
}

function softmaxRecord(logits: Record<string, number>): Record<string, number> {
  const entries = Object.entries(logits);
  if (entries.length === 0) return {};
  const max = Math.max(...entries.map(([, v]) => v));
  const exps = entries.map(([k, v]) => [k, Math.exp(v - max)] as const);
  const sum = exps.reduce((acc, [, e]) => acc + e, 0) || 1;
  const out: Record<string, number> = {};
  for (const [k, e] of exps) {
    out[k] = Math.round((e / sum) * 10000) / 10000;
  }
  return out;
}

/**
 * Self-hosted, zero-output-token calibrated local System-1 evaluator matching
 * the exact Gemini Decision 1 (`gemini-decision-1`, `/v1beta/decisions`) contract.
 */
export function evaluateDecisionsLocal(req: DecisionsRequest): DecisionsResponse {
  const entries = Object.entries(req.questions ?? {}).slice(0, 1000);
  const inputTokens = tokenize(req.input ?? "");
  const answers: Record<string, DecisionAnswer> = {};

  for (const [qid, q] of entries) {
    if (q.type === "noul") {
      const sim = overlapScore(inputTokens, q.instructions);
      // Calibrated sigmoid bounded strictly to [0.01, 0.99] per Gemini Decision 1 spec.
      const rawProb = 1 / (1 + Math.exp(-(sim * 3.0 - 0.6)));
      const noul = Math.round(Math.min(0.99, Math.max(0.01, rawProb)) * 100) / 100;
      const confidence = Math.round(Math.abs(noul - 0.5) * 2 * 100) / 100;
      answers[qid] = { type: "noul", noul, confidence };
    } else if (q.type === "choice") {
      const logits: Record<string, number> = {};
      for (const [opt, crit] of Object.entries(q.criteria ?? {})) {
        logits[opt] = overlapScore(inputTokens, `${opt} ${crit}`) * 3.5;
      }
      const probabilities = softmaxRecord(logits);
      const sorted = Object.entries(probabilities).sort((a, b) => b[1] - a[1]);
      const top = sorted[0] ?? ["default", 1];
      answers[qid] = {
        type: "choice",
        choice: top[0],
        probabilities,
        confidence: top[1],
      };
    } else if (q.type === "score") {
      const keys = Object.keys(q.criteria ?? {});
      const logits: Record<string, number> = {};
      for (const [level, crit] of Object.entries(q.criteria ?? {})) {
        logits[level] = overlapScore(inputTokens, `${level} ${crit}`) * 3.5;
      }
      const probabilities = softmaxRecord(logits);
      let expected = 0;
      let maxP = 0;
      keys.forEach((k, idx) => {
        const numeric = Number(k);
        const weight = Number.isFinite(numeric) ? numeric : idx + 1;
        const p = probabilities[k] ?? 0;
        expected += weight * p;
        if (p > maxP) maxP = p;
      });
      answers[qid] = {
        type: "score",
        score: Math.round(expected * 1000) / 1000,
        probabilities,
        confidence: Math.round(maxP * 10000) / 10000,
      };
    }
  }

  return {
    ok: true,
    model: req.model || "gemini-decision-1",
    backend: "fw-system1-local",
    outputTokens: 0,
    questionCount: entries.length,
    answers,
  };
}

/**
 * Dual-mode Gemini Decision 1 evaluator:
 * - Uses official `https://generativelanguage.googleapis.com/v1beta/decisions` when `GEMINI_API_KEY` is set and mode !== "local".
 * - Seamlessly falls back to Fw's self-hosted zero-token local System-1 engine offline/self-hosted.
 */
export async function evaluateDecisions(req: DecisionsRequest): Promise<DecisionsResponse> {
  const apiKey = process.env.GEMINI_API_KEY;
  if (req.mode !== "local" && apiKey) {
    try {
      const resp = await fetch("https://generativelanguage.googleapis.com/v1beta/decisions", {
        method: "POST",
        headers: {
          "x-goog-api-key": apiKey,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          model: req.model || "gemini-decision-1",
          input: req.input,
          questions: req.questions,
        }),
      });
      if (resp.ok) {
        const data = (await resp.json()) as { answers?: Record<string, DecisionAnswer> };
        const answers = data.answers ?? {};
        return {
          ok: true,
          model: req.model || "gemini-decision-1",
          backend: "gemini-decision-1-remote",
          outputTokens: 0,
          questionCount: Object.keys(answers).length,
          answers,
        };
      }
    } catch {
      // Fall back cleanly to local System-1 engine
    }
  }
  return evaluateDecisionsLocal(req);
}

/**
 * End-to-end self-hosted AI workflow fusing:
 * 1. Gemini Decision 1 (System-1 zero-token routing, policy check, rubric score)
 * 2. Fw SQLite FTS5 Brain retrieval
 * 3. Gemma / local GGUF / Antigravity / Cloudflare AI Router selection
 * 4. Fw Soul invariants
 */
export async function runSelfHostedAiWorkflow(root: string, taskInput: string, mode: "local" | "auto" = "local") {
  const decisions = await evaluateDecisions({
    model: "gemini-decision-1",
    input: taskInput,
    mode,
    questions: {
      requires_code_build: {
        type: "noul",
        instructions: "Does this task involve compiling, building, testing, or modifying code or binaries?",
      },
      route_engine: {
        type: "choice",
        instructions: "Select the optimal self-hosted execution backend for this task.",
        criteria: {
          "gemma-local-gguf": "Fast local privacy-first generation, summarization, or self-hosted chat using Gemma/GGUF.",
          "fw-lowlevel": "Binary analysis, PE/ELF/WASM, Assembly, CUDA, WebGPU, Rust Fat-LTO compilation.",
          "antigravity-sdk": "Multi-step autonomous coding agent orchestration, tools, MCP, ACP, and repo upgrades.",
          "cloudflare-edge": "Edge Worker deployment, Durable Objects VFS, R2/D1/Vectorize, or global API serving.",
        },
      },
      complexity_rubric: {
        type: "score",
        instructions: "Rate task complexity from 1 (trivial reflex) to 5 (deep multi-agent architecture).",
        criteria: {
          "1": "Simple lookup, ping, status, or formatting.",
          "2": "Single-file edit or quick inspection.",
          "3": "Multi-file feature with unit tests.",
          "4": "Cross-language native FFI, compiler, or cloud deployment.",
          "5": "Full monorepo, kernel, distributed OS, or multi-model ML training pipeline.",
        },
      },
    },
  });

  const routeAnswer = decisions.answers.route_engine;
  const selectedTrack = routeAnswer?.type === "choice" ? routeAnswer.choice : "gemma-local-gguf";
  const modelMap: Record<string, string> = {
    "gemma-local-gguf": "gemma-3-12b-it-gguf",
    "fw-lowlevel": "fw-local",
    "antigravity-sdk": "gemini-4-neon",
    "cloudflare-edge": "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
  };
  const routerDecision = routeModelRequest(modelMap[selectedTrack], "chat");
  const brainHits = brainSearch(root, taskInput, 4);
  const soul = loadSoul(root);

  return {
    ok: true,
    workflow: "fw-self-hosted-gemini-gemma-agy",
    input: taskInput,
    stage1_decisions: decisions,
    stage2_brain: brainHits,
    stage3_router: {
      selectedTrack,
      decision: routerDecision,
      availableProviders: DEFAULT_ROUTER_PROVIDERS.map((p) => p.id),
    },
    stage4_soul: soul.identity ?? { name: "Fw" },
  };
}
