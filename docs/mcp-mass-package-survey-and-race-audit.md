# MCP mass package survey and race-condition audit

MCPace must not become a hidden catalog that silently trusts package names. The mass survey is a registry-pressure test, not a preset list: it looks at many MCP-looking npm packages, classifies observable signals, and keeps every random package disabled until explicit operator review and safe live evidence exist.

## Mass package survey

`scripts/mcp-mass-package-survey.mjs` supports two modes:

- fixture replay for deterministic local checks;
- live npm metadata survey with `--live --limit 100`.

The live mode uses `npm search` metadata and can optionally download tarballs with `npm pack --ignore-scripts`. It does **not** start package bins, does **not** send `initialize` to random servers, and does **not** call `tools/call`. Optional install-lock benchmarking uses `npm install --package-lock-only --ignore-scripts`; it is intentionally reported separately because resolving 100 arbitrary packages may be slow or impossible in restricted mirrors.

The policy output is deliberately conservative:

- unknown stdio packages stay `review-required-single-writer`;
- credential/cloud/admin/browser/shell packages stay disabled/review-gated;
- filesystem/git/database packages get project/repo/db single-writer locks;
- memory/context packages get session/context locks;
- read-only-looking utilities are only candidates until safe probes confirm behavior.


### 100-package install-lock pressure

There are two install-lock lanes:

```bash
npm run benchmark:mcp-mass-package-install-lock
npm run benchmark:mcp-mass-package-install-lock:chunked
```

The first lane intentionally tries the all-at-once dependency graph and may be
reported as blocked when npm resolution exceeds the host budget. The chunked lane
keeps the same 100-package target but splits it with
`--resolve-install-lock-chunks`, so a single pathological package group does not
hide the rest of the ecosystem behavior. A bounded smoke form can stop after the
first chunks with `--resolve-install-lock-max-chunks`; partial reports are marked
`blocked` and include `attemptedPackages`, `remainingPackages`, and failed chunk
labels. This is not used to auto-enable servers. It is only pressure evidence.

## Race-condition audit

`scripts/mcp-race-condition-audit.mjs` fuzzes scheduler decisions across multiple clients, chats, sessions, projects, credentials, remote transport sessions, browser contexts, and provider budgets. It asserts that:

- disabled servers block before scheduling;
- unknown/high-risk profiles block behind review gates;
- unadvertised tools block before forwarding;
- same-project filesystem/git/database work does not overlap on exclusive locks;
- memory/session and remote Streamable HTTP session affinity stays separated;
- credentialed API work never shares a worker pool without credential/tenant affinity.

This audit complements, but does not replace, native Rust tests. Public release proof still requires a Rust host running the Rust test lanes and rebuilding the binary.
