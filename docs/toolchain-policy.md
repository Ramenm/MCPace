# Toolchain Policy

`reports/toolchain-support.json` is the machine-readable source of truth for the
maintained stack. Update that file, the manifests, the local version files, the
CI workflow, and the docs together.

## Goals

- keep contributor machines and CI on maintained support lanes;
- keep the npm surface launcher-only and honest about what it can prove;
- keep Rust bug reports reproducible with a pinned compiler;
- keep command-family source files small by preferring thin module roots plus
  focused submodules;
- keep clean release packaging repeatable with one archive builder script instead
  of manual zip assembly.

## Node and npm policy

- supported contributor and CI lanes are **Node 22 LTS** and **Node 24 LTS**;
- the default local-development line is **Node 24**, recorded in **`.nvmrc`** and
  **`.node-version`**;
- the repo engine floor is **`>=22.0.0`** and the workspace expects **npm 10+** while pinning **`npm@11.12.1`** as the default `packageManager`;
- `package.json` keeps a pinned `packageManager` plus `devEngines` ranges so
  unsupported stacks fail fast instead of drifting silently;
- future platform-specific binary packages should declare `os`, `cpu`, and
  `libc` explicitly instead of guessing support at install time.

## Rust policy

- `rust-toolchain.toml` is pinned to **Rust 1.95.0** with the **minimal** profile;
- `rustfmt` and `clippy` travel with the pinned toolchain;
- the crate still uses **edition 2021** today; do not flip to edition 2024 until
  Linux, Windows, and macOS build proof is rerun and recorded;
- the manifest intentionally stays dependency-light so source proof does not
  depend on a large online dependency graph.

## CI policy

- GitHub Actions use **`actions/checkout@v6`** and **`actions/setup-node@v6`**;
- Node source validation runs a slim matrix: **Ubuntu** carries both maintained
  Node majors, while **Windows** and **macOS** run the default local line;
- npm package dry-run proof is a separate Ubuntu job that resolves Node from
  **`.nvmrc`**;
- Rust build proof runs on **Ubuntu, Windows, and macOS** with the pinned
  toolchain;
- the workflow uses read-only permissions and cancels superseded runs so the CI
  surface stays cheaper to maintain.

## Maintenance policy

- Dependabot watches **GitHub Actions**, **npm**, and **Cargo** metadata weekly;
- recheck Node lanes whenever an LTS line changes status or reaches EOL;
- repin Rust only after a host-verified pass;
- keep packaging honesty ahead of convenience: if a lane is not published or not
  verified, do not advertise it as supported.

## Release archive policy

- `scripts/archive-release.mjs` is the canonical builder for source release zip files;
- generated roots must follow `<project-name>-v<version>-<ddmmyy-hhmmss>`;
- archive contents must come from `release-manifest.json` and stay free of `.git`,
  `node_modules`, `target`, caches, and OS junk.
