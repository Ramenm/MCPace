# MCPace bug sweep

Project: `mcpace` v`0.6.5`
Status: **pass**

## Summary

- Checks: 13
- Blocked: 0
- Warnings: 0

## Checks

| Check | Severity | Status | Evidence | Next action |
|---|---:|---:|---|---|
| `file:docs/bug-hunting-and-fix-playbook.md` | info | pass | the reproducible bug-fix lifecycle; 155 lines | Keep this invariant covered while changing nearby code. |
| `file:docs/defect-taxonomy-and-labels.md` | info | pass | maintainer triage labels and severity routing; 69 lines | Keep this invariant covered while changing nearby code. |
| `file:docs/maintainer-debugging-guide.md` | info | pass | area-specific debugging commands; 67 lines | Keep this invariant covered while changing nearby code. |
| `script:verify:bug-sweep` | info | pass | node scripts/bug-sweep.mjs --json --write reports/bug-sweep-latest.json --markdown reports/bug-sweep-latest.md | Keep this invariant covered while changing nearby code. |
| `workflow:ci-bug-sweep` | info | pass | CI runs verify:bug-sweep in the source validation lane. | Keep this invariant covered while changing nearby code. |
| `template:pr-bug-fix-discipline` | info | pass | PR template requires root cause, regression proof, and not-tested disclosure. | Keep this invariant covered while changing nearby code. |
| `template:bug-report-repro` | info | pass | Bug template captures reproduction and regression context. | Keep this invariant covered while changing nearby code. |
| `template:flaky-test` | info | pass | Dedicated flaky-test issue form is present. | Keep this invariant covered while changing nearby code. |
| `source:prod-rust-stub-macros` | info | pass | No todo!, unimplemented!, or dbg! macros found in production Rust files. | Keep this invariant covered while changing nearby code. |
| `runtime:http-origin-host-boundary` | info | pass | Local HTTP boundary validates Host and Origin and explicitly rejects Origin: null by default. | Keep this invariant covered while changing nearby code. |
| `runtime:server-minted-session-id` | info | pass | Initialize path uses a server-minted session id. | Keep this invariant covered while changing nearby code. |
| `runtime:session-id-bounds-randomness` | info | pass | Session ids are bounded and generated from OS randomness without insecure fallback. | Keep this invariant covered while changing nearby code. |
| `reports:runtime-trace-present` | info | pass | runtime trace report is present and passing. | Keep this invariant covered while changing nearby code. |
