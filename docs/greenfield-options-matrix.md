# Greenfield options matrix

## Decision question

How should MCPace move toward a lightweight single-hub Rust product?

## Options

| Option | Implementation | Support burden | Main risks | Performance | Compatibility | License / cost |
|---|---|---|---|---|---|---|
| **Greenfield core inside the current repo** | Moderate. New architecture can be built without losing repo history or the existing bridge. | Moderate. One repo, one backlog, one release story. | Requires discipline so greenfield work does not silently drift from current runtime contract. | Strong. Native Rust target remains intact. | Strongest migration path because old and new can coexist. | No extra repo/licensing cost. |
| **Separate fork / new repo immediately** | High. Requires bootstrapping a second codebase and new packaging/test/docs/release machinery before parity exists. | High. Two repos drift unless cutover is immediate. | Architecture may look cleaner, but the migration cost and status confusion grow sharply. | Strong once mature. | Weakest short-term compatibility because bridge/cutover coordination becomes harder. | No runtime fee, but higher maintenance cost. |
| **Keep only incremental command-by-command port inside legacy shape** | Lowest immediate cost. | Moderate. Easier to land tiny slices. | Risk of preserving the old command model and never reaching the simpler product shape. | Strong enough. | Good short-term compatibility, weaker long-term simplification. | No extra cost. |

## Recommendation

Choose **greenfield core inside the current repo**.

Why:

- it preserves the current compatibility bridge
- it allows a new command model without creating a second public project prematurely
- it keeps docs, evals, release hygiene, and migration state in one place
- it still allows a future repo split once crate boundaries and product boundaries are proven

## Plan B

If the new crate boundaries become clean and the compatibility bridge shrinks to a thin adapter, re-evaluate a repo split later.
Do **not** fork first and design second.
