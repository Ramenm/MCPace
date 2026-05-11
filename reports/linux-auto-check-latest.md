# MCPace Linux auto-check report

Status: **warn**
Profile: **host**
Generated: 2026-05-11T11:37:56.743Z
Root: `/mnt/data/mcpace_ideal_final_source`

| Status | Check | Message |
|---|---|---|
| pass | linux-platform | platform=linux |
| pass | linux-architecture | arch=x64 |
| pass | linux-libc | libc=gnu |
| pass | node-runtime | node=22.16.0 |
| pass | command-npm | /opt/nvm/versions/node/v22.16.0/bin/npm |
| pass | command-npx | /opt/nvm/versions/node/v22.16.0/bin/npx |
| pass | command-sh | /usr/bin/sh |
| warn | optional-command-cargo | cargo not found |
| warn | optional-command-rustc | rustc not found |
| warn | optional-command-docker | docker not found |
| pass | optional-command-systemctl | /usr/bin/systemctl |
| pass | package-json | package.json present and parseable |
| pass | cargo-manifest | Cargo.toml present |
| pass | release-targets | release-targets.json present |
| pass | npm-lint | npm run lint:npm -- --json passed |
| pass | npm-cli-tests | npm run test:npm passed |
| pass | release-targets-gate | npm run verify:release-targets passed |
| pass | platform-packages-gate | npm run verify:platform-packages passed |
| pass | npm-pack-gate | npm run verify:npm-pack passed |
| pass | xdg-config-root-parent-writable | writable: /mnt/data |
| pass | xdg-state-root-parent-writable | writable: /home/oai/.local/state |
| pass | serve-host-local-only | serve host=127.0.0.1 |
| warn | systemd-user | Failed to connect to user scope bus via local transport: Is a directory |
| pass | npx-upstream-env-vars | npx upstream env_vars look configured or no npx upstreams enabled |
| pass | inline-server-env-secrets | no inline env blocks detected in enabled npx upstreams |
| warn | mcpace-binary | mcpace binary not found in --bin/PATH/target |
| pass | release-manifest-hygiene | release manifest excludes local/private machine-state paths |
| pass | archive-release-exclusions | archive-release excludes known local machine-state directories |
| pass | vendored-executable-bits | vendored Unix binaries are executable |
| pass | linux-npm-install-docker-script | test:linux-npm-install:docker => node scripts/verify-linux-npm-install-docker.mjs --json |
| skip | linux-npm-install-docker-proof | Docker proof not executed for this profile |

## Next actions
- WARN optional-command-cargo: cargo not found
- WARN optional-command-rustc: rustc not found
- WARN optional-command-docker: docker not found
- WARN systemd-user: Failed to connect to user scope bus via local transport: Is a directory
- WARN mcpace-binary: mcpace binary not found in --bin/PATH/target
