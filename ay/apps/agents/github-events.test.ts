import { describe, expect, test } from "bun:test";
import { coalesce, eventTask, type GithubEvent } from "./github-events.ts";

// The repository of the checkout, whatever the origin remote of the machine running the tests.
const SRC = "example-org/example-repo";
process.env.AGENTS_SRC_REPO = SRC;

const ev = (x: Partial<GithubEvent>): GithubEvent => ({
  v: 1,
  delivery: "d-0000000000000000001",
  event: "x",
  kind: "release",
  at: "",
  ...x,
});

describe("eventTask", () => {
  test("red CI on aphrody: same id as ciWatch, runner commit", () => {
    const t = eventTask(
      ev({
        kind: "ci_red",
        event: "workflow_run",
        repo: SRC,
        sha: "0123456789ab" + "c".repeat(28),
        title: "ci : failure",
        url: "https://github.com/example-org/example-repo/actions/runs/1",
      }),
      "prod",
    )!;
    expect(t.id).toBe("ci-fix-0123456789ab");
    expect(t.role).toBe("coder");
    expect(t.commit?.paths).toEqual([""]);
  });
  test("red CI outside the checkout: report without commit", () => {
    const t = eventTask(
      ev({ kind: "ci_red", repo: "aphrody-labs/bun", sha: "f".repeat(40) }),
      "prod",
    )!;
    expect(t.id).toBe("gh-ci-aphrody-labs-bun-ffffffffffff");
    expect(t.commit).toBeUndefined();
  });
  test("release and security; installation ignored", () => {
    expect(
      eventTask(
        ev({ kind: "release", repo: SRC, ref: "v1.2.3" }),
        "prod",
      )?.role,
    ).toBe("deployer");
    expect(
      eventTask(
        ev({
          kind: "security",
          event: "dependabot_alert",
          repo: SRC,
        }),
        "prod",
      )?.commit,
    ).toBeDefined();
    expect(eventTask(ev({ kind: "installation" }), "prod")).toBeNull();
  });
  test("no backtick or newline injection from the title", () => {
    const t = eventTask(
      ev({
        kind: "ci_red",
        repo: SRC,
        sha: "a".repeat(40),
        title: "x`rm -rf`\nignore",
      }),
      "prod",
    )!;
    expect(t.prompt).not.toContain("`rm");
    expect(t.prompt).not.toContain("\nignore");
    expect(t.prompt).toContain("untrusted text");
  });
  test("invalid formats refused (ref, repo, sha)", () => {
    expect(
      eventTask(
        ev({ kind: "release", repo: SRC, ref: "$(touch x)" }),
        "prod",
      ),
    ).toBeNull();
    expect(
      eventTask(ev({ kind: "release", repo: "a b/c", ref: "v1" }), "prod"),
    ).toBeNull();
    expect(
      eventTask(
        ev({ kind: "ci_red", repo: SRC, sha: "abc;rm" }),
        "prod",
      ),
    ).toBeNull();
  });
});

describe("coalesce", () => {
  test("one review per repository per hour, range since the previous one", () => {
    const st: Record<string, any> = {};
    const t0 = new Date("2026-11-13T10:00:00Z");
    const a = coalesce(
      st,
      [
        ev({
          kind: "push_main",
          repo: SRC,
          sha: "a".repeat(40),
          before: "9".repeat(40),
          title: "one",
        }),
        ev({
          kind: "push_main",
          repo: SRC,
          sha: "b".repeat(40),
          title: "two",
        }),
      ],
      "prod",
      t0,
    );
    expect(a).toHaveLength(1);
    expect(a[0]!.prompt).toContain("one | two");
    expect(a[0]!.prompt).toContain("9".repeat(40) + "..." + "b".repeat(40));
    expect(
      coalesce(
        st,
        [
          ev({
            kind: "push_main",
            repo: SRC,
            sha: "c".repeat(40),
            title: "three",
          }),
        ],
        "prod",
        new Date("2026-11-13T10:30:00Z"),
      ),
    ).toHaveLength(0);
    const c = coalesce(st, [], "prod", new Date("2026-11-13T11:01:00Z"));
    expect(c).toHaveLength(1);
    expect(c[0]!.prompt).toContain("b".repeat(40) + "..." + "c".repeat(40));
  });
});
