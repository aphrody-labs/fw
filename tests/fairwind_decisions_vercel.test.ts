import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { createFairwindApiHandler } from "../src/api.ts";
import { evaluateDecisionsLocal, runSelfHostedAiWorkflow } from "../src/decisions.ts";
import { GET as vercelGet, POST as vercelPost } from "../vercel/api/fairwind.ts";

const ROOT = process.cwd();

describe("fairwind Gemini Decision 1, self-hosted workflow, Antigravity SDK fork, Auto core, and Vercel cross-cloud", () => {
  test("forked Antigravity Python SDK, Auto core, fairwind-kernel, and fairwind-world are present", () => {
    expect(existsSync(join(ROOT, "core", "py", "google"))).toBe(true);
    expect(existsSync(join(ROOT, "core", "rs", "auto", "auto-auth", "Cargo.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "native", "fairwind-kernel", "Cargo.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "native", "fairwind-world", "Cargo.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "vercel", "vercel.json"))).toBe(true);
  });

  test("Gemini Decision 1 local zero-token engine evaluates noul, choice, and score questions", () => {
    const res = evaluateDecisionsLocal({
      model: "gemini-decision-1",
      input: "The blue mug I ordered on Monday arrived cracked and damaged on Wednesday.",
      questions: {
        is_damaged: { type: "noul", instructions: "Did the item arrive cracked or damaged?" },
        queue: {
          type: "choice",
          instructions: "Which support queue should handle this message?",
          criteria: {
            billing: "Payment, invoice, or refund questions.",
            replacements: "Damaged, cracked, missing, or wrong items.",
            general: "Product questions and account updates.",
          },
        },
        severity: {
          type: "score",
          instructions: "Rate issue severity from 1 to 3.",
          criteria: {
            "1": "General account question",
            "2": "Minor delay",
            "3": "Damaged or cracked item needing replacement",
          },
        },
      },
    });
    expect(res.ok).toBe(true);
    expect(res.outputTokens).toBe(0);
    expect(res.questionCount).toBe(3);
    const dmg = res.answers.is_damaged;
    const queue = res.answers.queue;
    const sev = res.answers.severity;
    expect(dmg.type).toBe("noul");
    if (dmg.type === "noul") {
      expect(dmg.noul).toBeGreaterThanOrEqual(0.01);
      expect(dmg.noul).toBeLessThanOrEqual(0.99);
    }
    expect(queue.type).toBe("choice");
    if (queue.type === "choice") expect(queue.choice).toBe("replacements");
    expect(sev.type).toBe("score");
    if (sev.type === "score") expect(sev.score).toBeGreaterThan(2.0);
  });

  test("end-to-end self-hosted AI workflow and /v1beta/decisions + Vercel handler work", async () => {
    const wf = await runSelfHostedAiWorkflow(ROOT, "Compile Rust Fat-LTO PE binary and WebAssembly kernel", "local");
    expect(wf.ok).toBe(true);
    expect(wf.stage1_decisions.outputTokens).toBe(0);
    expect(wf.stage3_router.selectedTrack).toBe("fairwind-lowlevel");

    const handler = createFairwindApiHandler(ROOT);
    const apiDec = (await (
      await handler(
        new Request("http://127.0.0.1:4379/v1beta/decisions", {
          method: "POST",
          body: JSON.stringify({
            model: "gemini-decision-1",
            mode: "local",
            input: "Deploy edge worker on Cloudflare R2 and D1",
            questions: {
              is_cloud: { type: "noul", instructions: "Cloudflare edge worker deployment" },
            },
          }),
        }),
      )
    ).json()) as { ok: boolean; outputTokens: number };
    expect(apiDec.ok).toBe(true);
    expect(apiDec.outputTokens).toBe(0);

    const vg = (await (await vercelGet(new Request("https://fairwind.vercel.app/health"))).json()) as { ok: boolean; runtime: string };
    expect(vg.ok).toBe(true);
    expect(vg.runtime).toBe("vercel-fluid-bun1.4");

    const vp = (await (
      await vercelPost(
        new Request("https://fairwind.vercel.app/v1beta/decisions", {
          method: "POST",
          body: JSON.stringify({
            model: "gemini-decision-1",
            mode: "local",
            input: "Payment invoice refund question",
            questions: {
              q: {
                type: "choice",
                instructions: "Route ticket",
                criteria: { billing: "Payment invoice refund", tech: "Server crash" },
              },
            },
          }),
        }),
      )
    ).json()) as { ok: boolean; answers: { q: { choice: string } } };
    expect(vp.ok).toBe(true);
    expect(vp.answers.q.choice).toBe("billing");
  });
});
