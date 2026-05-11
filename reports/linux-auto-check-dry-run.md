# MCPace Linux auto-check report

Status: **warn**
Profile: **host**
Generated: 2026-05-11T10:55:58.693Z
Root: `/mnt/data/mcpace_linux_human_v3`

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
| skip | npm-lint | dry-run: npm run lint:npm -- --json |
| skip | npm-cli-tests | dry-run: npm run test:npm |
| skip | release-targets-gate | dry-run: npm run verify:release-targets |
| skip | platform-packages-gate | dry-run: npm run verify:platform-packages |
| skip | npm-pack-gate | dry-run: npm run verify:npm-pack |
| pass | xdg-config-root-parent-writable | writable: /mnt/data |
| pass | xdg-state-root-parent-writable | writable: /home/oai/.local/state |
| pass | serve-host-local-only | serve host=127.0.0.1 |
| warn | systemd-user | Failed to connect to user scope bus via local transport: Is a directory |
| pass | npx-upstream-env-vars | npx upstream env_vars look configured or no npx upstreams enabled |
| pass | inline-server-env-secrets | no inline env blocks detected in enabled npx upstreams |
| warn | mcpace-binary | mcpace binary not found in --bin/PATH/target |
| warn | release-manifest-hygiene | root-level screenshots are present; remove them from source snapshots even when release manifest excludes them |
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
- WARN release-manifest-hygiene: root-level screenshots are present; remove them from source snapshots even when release manifest excludes them
