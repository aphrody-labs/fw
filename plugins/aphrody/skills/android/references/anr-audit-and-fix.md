<!-- Merged from skill `anr-audit-and-fix` into `android`. Original frontmatter:
name: "anr-audit-and-fix"
description: "Audits Android codebases for ANR risks and diagnoses ANR stack dumps. Use when scanning Android code for ANR risks or triaging ANR stack dumps. Don't use for non-ANR crashes, dropped frames"
-->
# anr-audit-and-fix

> Audits Android codebases for ANR risks and diagnoses ANR stack dumps. Use when scanning Android code for ANR risks or triaging ANR stack dumps. Don't use for non-ANR crashes, dropped frames

> general CPU profiling, or Perfetto system traces.


Select the execution path matching the user's request:

## 1. Path A: Static codebase ANR risk audit

When auditing source files or scanning for thread-blocking operations:

- Follow [references/anr-audit-and-fix/references/risk_analysis_guide.md](references/anr-audit-and-fix/references/risk_analysis_guide.md) step by step: list candidates, **prove the executing thread** (trace callers when the thread depends on them), **prove the call blocks**, then attribute and grade. Report only candidates that pass these checks; list the dismissed ones separately with a one-line reason.
- Evaluate off-main-thread mutations of attached Views not merely as crash risks (`CalledFromWrongThreadException`), but as **Sync Barrier Leak ANR** risks with their own proof (off-main execution plus an attached View).

## 2. Path B: Trace-driven diagnosis and remediation

When analyzing ANR traces or stack dumps:

- Diagnose systematically using [references/anr-audit-and-fix/references/trace_analysis_guide.md](references/anr-audit-and-fix/references/trace_analysis_guide.md) and format the report with [references/anr-audit-and-fix/references/anr_trace_report_template.md](references/anr-audit-and-fix/references/anr_trace_report_template.md).
- Base findings strictly on log evidence. Propose application fixes **only** for `APP_DEFECT` or `APP_RESOURCE_OVERLOAD` (never for `EXTERNAL_BOTTLENECK`).
