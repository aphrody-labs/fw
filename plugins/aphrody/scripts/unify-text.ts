// Pure text transforms of the unify generator: lenient frontmatter, description compaction, legacy-tool rewriting.
import { rewriteCommand } from "../hooks/rewrite.ts";

export interface Skill {
  frontmatter: Record<string, string>;
  body: string;
}

const FRONTMATTER = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
const KEY = /^([A-Za-z_][\w-]*):\s*(.*)$/;

const unquote = (value: string): string => {
  const text = value.trim();
  if (text.length >= 2 && (text[0] === '"' || text[0] === "'") && text.at(-1) === text[0]) {
    try {
      return text[0] === '"' ? (JSON.parse(text) as string) : text.slice(1, -1).replaceAll("''", "'");
    } catch {
      return text.slice(1, -1);
    }
  }
  return text;
};

/** Top-level `key: value` pairs of a frontmatter block; tolerates unquoted colons that strict YAML rejects. */
export function parseSkill(text: string): Skill {
  const match = FRONTMATTER.exec(text);
  if (!match) return { frontmatter: {}, body: text };
  const frontmatter: Record<string, string> = {};
  let key = "";
  let chunks: string[] = [];
  let folded = false;
  const flush = () => {
    if (!key) return;
    const joined = chunks.map((line) => line.trim()).filter((line) => line !== "" || !folded);
    const items = joined.filter((line) => line !== "");
    frontmatter[key] = items.length > 0 && items.every((line) => line.startsWith("- ")) ? items.map((line) => line.slice(2).trim()).join(", ") : unquote(joined.join(" ").trim());
  };
  for (const line of (match[1] ?? "").split(/\r?\n/)) {
    const top = !/^\s/.test(line) ? KEY.exec(line) : null;
    if (top) {
      flush();
      key = top[1] ?? "";
      const value = top[2] ?? "";
      folded = /^[>|][+-]?$/.test(value.trim());
      chunks = folded ? [] : [value];
    } else if (key) chunks.push(line);
  }
  flush();
  return { frontmatter, body: text.slice(match[0].length) };
}

const KEEP_KEYS = new Set([
  "name",
  "description",
  "allowed-tools",
  "argument-hint",
  "disable-model-invocation",
  "user-invocable",
  "model",
  "effort",
  "context",
  "agent",
  "paths",
  "when_to_use",
]);

export function renderSkill(skill: Skill): string {
  const lines = ["---"];
  for (const [key, value] of Object.entries(skill.frontmatter)) {
    if (KEEP_KEYS.has(key)) lines.push(`${key}: ${JSON.stringify(value)}`);
  }
  return `${lines.join("\n")}\n---\n${skill.body.replace(/^\s*\n/, "")}`;
}

const OPENERS: Array<[RegExp, string]> = [
  [/\b(?:Use|Used) this skill (?=when|whenever|if|any time|to|for|before|alongside)/gi, "Use "],
  [/\bThis skill (?:should be used|is used|is required|must be used|is mandatory)\s+(?=when|whenever|to|for|before)/gi, "Use "],
  [/\*\*MANDATORY prerequisite\*\*\s*[-—:]*\s*/gi, "Prerequisite: "],
  [/\bMANDATORY prerequisite\s*[-—:]*\s*/gi, "Prerequisite: "],
];

/** Longest prefix of `text` that fits `limit`, cut at a clause end, else a word boundary. */
function fit(text: string, limit: number): { head: string; tail: string } {
  if (text.length <= limit) return { head: text, tail: "" };
  const window = text.slice(0, limit);
  const clause = Math.max(window.lastIndexOf(", "), window.lastIndexOf("; "), window.lastIndexOf(" - "));
  const cut = clause > limit * 0.5 ? clause : window.lastIndexOf(" ");
  const head = text.slice(0, cut > 0 ? cut : limit).replace(/[\s,;:-]+$/, "");
  return { head, tail: text.slice(head.length).trim() };
}

/**
 * One tight line: markup and openers stripped; when it is still too long the "Use when ..." trigger sentence is kept
 * (it decides whether the skill loads) and the leading summary is cut to what remains. `rest` is what was cut.
 */
export function compactDescription(description: string, max: number): { line: string; rest: string } {
  let text = description.replace(/\*\*|`/g, "").replace(/\s+/g, " ").trim();
  for (const [pattern, replacement] of OPENERS) text = text.replace(pattern, replacement);
  text = text.replace(/\s+([,.;:])/g, "$1").trim();
  if (text.length <= max) return { line: text, rest: "" };
  const sentences = text.split(/(?<=[.;])\s+(?=[A-Z])/);
  const trigger = sentences.findIndex((sentence, index) => index > 0 && /^(?:Use |Used |Trigger|Activate|Invoke|When |Whenever )/.test(sentence));
  if (trigger < 0) {
    const { head, tail } = fit(text, max);
    return { line: head, rest: tail };
  }
  const summary = sentences.slice(0, trigger).join(" ");
  const when = sentences.slice(trigger).join(" ");
  const whenFit = fit(when, Math.max(Math.floor(max * 0.55), max - summary.length - 1));
  const summaryFit = fit(summary, max - whenFit.head.length - 1);
  const head = /[.;]$/.test(summaryFit.head) || summaryFit.head === "" ? summaryFit.head : `${summaryFit.head}.`;
  const line = `${head} ${whenFit.head}`.trim();
  const rest = [summaryFit.tail, whenFit.tail].filter(Boolean).join(" ... ").replace(/\s*[.,;]\s+(?=[,;])/g, " ").replace(/(^|\s)[,;:.-]+\s/g, "$1");
  return { line, rest };
}

const TOOL_PERMISSIONS: Array<[RegExp, string]> = [
  [/Bash\(npx(?=[: ])/g, "Bash(bunx"],
  [/Bash\((?:npm|pnpm|yarn|node)(?=[: ])/g, "Bash(bun"],
  [/Bash\((?:pip3?|python3?)(?=[: ])/g, "Bash(uv"],
];

const SHELL_LANGUAGES = new Set(["", "bash", "sh", "shell", "zsh", "console", "sh-session", "shellsession", "powershell", "ps1", "pwsh", "bat", "cmd", "text", "txt"]);
const LEGACY_HEAD = /^(?:\$\s+|>\s+|PS>\s+)?(?:npm|npx|pnpx|yarn|pnpm|node|nodejs|pip3?|pipx|python3?|py|jest|vitest|mocha|eslint|prettier)\b/;
// A line that already names the target or compares tools documents the mapping; rewriting it would erase the point.
const MAPPING_LINE = /\b(?:bun|bunx|uv|uvx)\b|->|→|\binstead of\b|\brather than\b|\bnever\b|\bvs\.?\b/i;

const AT_SEGMENT = (pattern: string): RegExp => new RegExp(`(^|&&\\s*|\\|\\|\\s*|;\\s*)${pattern}`, "g");

const UV_RULES: Array<[RegExp, string]> = [
  [AT_SEGMENT("(?:python3?|py)\\s+-m\\s+pip3?\\b"), "$1uv pip"],
  [AT_SEGMENT("(?:python3?|py)\\s+-m\\s+venv\\b"), "$1uv venv"],
  [AT_SEGMENT("pip3?\\b"), "$1uv pip"],
  [AT_SEGMENT("pipx\\s+run\\b"), "$1uvx"],
  [AT_SEGMENT("pipx\\s+install\\b"), "$1uv tool install"],
  [AT_SEGMENT("(?:python3?|py)\\s+(-c|-m)\\b"), "$1uv run python $2"],
  [AT_SEGMENT("(?:python3?|py)\\s+(?=[^-\\s])"), "$1uv run "],
  [AT_SEGMENT("(?:(?:npx|bunx)\\s+(?:-y\\s+|--yes\\s+)?)?(?:jest|vitest|mocha)\\b"), "$1bun test"],
];

const uvSegments = (command: string): string => UV_RULES.reduce((acc, [pattern, replacement]) => acc.replace(pattern, replacement), command);

/** The command in the Aphrody toolchain, or undefined when it already is (or is not a legacy tool call). */
export function toolchainCommand(command: string): string | undefined {
  if (MAPPING_LINE.test(command) && !/^(?:\$\s+)?(?:npx|npm|yarn|pnpm|pip3?|python3?|node)\b[^|&;]*$/.test(command.trim())) return undefined;
  const prompt = /^(\$\s+|>\s+|PS>\s+)/.exec(command)?.[0] ?? "";
  const bare = command.slice(prompt.length);
  const first = rewriteCommand(bare, { aphrody: false })?.command ?? bare;
  const second = uvSegments(first).replace(/^bun (?:install|i)\b(?=(?:\s+-\S+)*\s+[^-\s])/, "bun add");
  return second === bare ? undefined : prompt + second;
}

export interface Rewritten {
  text: string;
  changes: number;
}

/** Rewrites shell code blocks and inline code spans of a markdown file; prose and other languages stay untouched. */
export function rewriteMarkdown(text: string): Rewritten {
  let changes = 0;
  const apply = (command: string): string => {
    const next = LEGACY_HEAD.test(command.trim()) ? toolchainCommand(command) : undefined;
    if (next === undefined) return command;
    changes++;
    return next;
  };
  let fence: { marker: string; shell: boolean } | undefined;
  const out = text.split("\n").map((line) => {
    const opening = /^(\s*)(`{3,}|~{3,})\s*([\w+-]*)/.exec(line);
    if (opening) {
      if (!fence) fence = { marker: opening[2] ?? "```", shell: SHELL_LANGUAGES.has((opening[3] ?? "").toLowerCase()) };
      else if ((opening[2] ?? "").startsWith(fence.marker[0] ?? "`") && !opening[3]) fence = undefined;
      return line;
    }
    if (MAPPING_LINE.test(line) && !/^\s*(?:\$\s+)?(?:npx|npm|yarn|pnpm|pip3?|python3?|node)\b[^|&;]*$/.test(line)) return line;
    if (fence) {
      if (!fence.shell) return line;
      const indent = /^\s*/.exec(line)?.[0] ?? "";
      return indent + apply(line.slice(indent.length));
    }
    return line.replace(/`([^`\n]+)`/g, (whole, span: string) => {
      const next = apply(span);
      return next === span ? whole : `\`${next}\``;
    });
  });
  return { text: out.join("\n"), changes };
}

export function rewriteFrontmatterPermissions(value: string): string {
  return TOOL_PERMISSIONS.reduce((acc, [pattern, replacement]) => acc.replace(pattern, replacement), value);
}

export function rewriteShebang(text: string): string {
  return text
    .replace(/^#!\s*\/usr\/bin\/env\s+(?:-S\s+)?node\b.*$/m, "#!/usr/bin/env bun")
    .replace(/^#!\s*\/usr\/bin\/env\s+python3?\b.*$/m, "#!/usr/bin/env -S uv run --script");
}

/** Replaces (or adds) the description of a frontmatter block, leaving every other line byte for byte. */
export function replaceDescription(text: string, description: string): string {
  const match = FRONTMATTER.exec(text);
  if (!match) return text;
  const kept: string[] = [];
  let skipping = false;
  let placed = false;
  const line = `description: ${JSON.stringify(description)}`;
  for (const current of (match[1] ?? "").split(/\r?\n/)) {
    if (/^description:/.test(current)) {
      kept.push(line);
      placed = skipping = true;
      continue;
    }
    if (skipping && (/^\s/.test(current) || current === "")) continue;
    skipping = false;
    kept.push(current);
  }
  if (!placed) kept.push(line);
  return `---\n${kept.join("\n")}\n---\n${text.slice(match[0].length)}`;
}

const LOOSE: Array<[RegExp, string]> = [
  [/\b(?:npx|pnpx)\s+(?:-y\s+|--yes\s+)?(?:tsx|ts-node)\s+/g, "bun "],
  [/\b(?:npx|pnpx)\s+(?:-y\s+|--yes\s+)?(?:jest|vitest|mocha)\b/g, "bun test"],
  [/\b(?:npx|pnpx)\s+(?:-y\s+|--yes\s+)?(?:eslint)\b/g, "bun lint"],
  [/\b(?:npx|pnpx)\s+(?:-y\s+|--yes\s+)?(?:prettier)\b/g, "bun fmt"],
  [/\b(?:npx|pnpx)\s+(?:-y\s+|--yes\s+)?(?=[@\w.-])/g, "bunx "],
  [/\b(?:npm|pnpm|yarn)\s+(?:install|i|add)\s+(?:(?:-D|--save-dev|--save|-S)\s+)?(?=[@\w.-])/g, "bun add "],
  [/\b(?:npm|pnpm)\s+(?:install|i|ci)\b/g, "bun install"],
  [/\b(?:npm|pnpm)\s+run\s+/g, "bun run "],
  [/\bnpm\s+(test|start)\b/g, "bun run $1"],
  [/\bpip3?\s+install\b/g, "uv pip install"],
  [/\bpython3?\s+-m\s+venv\b/g, "uv venv"],
];

/** Usage strings and comments inside scripts, examples and prose: same toolchain, no command parsing. */
export function rewriteLoose(text: string): Rewritten {
  let changes = 0;
  const out = text
    .replace(/^#!\s*\/usr\/bin\/env\s+npx\s+tsx.*$/m, () => {
      changes++;
      return "#!/usr/bin/env bun";
    })
    .split("\n")
    .map((line) => {
      if (MAPPING_LINE.test(line)) return line;
      let next = line;
      for (const [pattern, replacement] of LOOSE) next = next.replace(pattern, replacement);
      if (next !== line) changes++;
      return next;
    });
  return { text: out.join("\n"), changes };
}
