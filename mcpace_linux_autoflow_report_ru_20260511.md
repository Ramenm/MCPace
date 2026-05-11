# MCPace Linux autoflow audit / hardening report — 2026-05-11

## Короткий вывод

Я сделал отдельный Linux-проход: не только “код компилируется”, а именно human flow: обычный пользователь ставит, запускает setup, получает локальный MCP endpoint, получает autostart, потом проверяет upstream MCP-серверы и release/npm безопасность.

Главный новый баг: Linux autostart был file-only. Код писал `~/.config/systemd/user/mcpace.service`, но не доказывал реальный `systemctl --user enable`. Для systemd это не полноценный автозапуск. Исправление в патче переводит Linux auto-launch на реальный user-unit lifecycle: write unit → daemon-reload → enable → is-enabled → restart/health smoke.

## Что доделано

- Добавлен `scripts/linux-auto-check.mjs`: единый JSON/Markdown gate для Linux.
- Добавлен `scripts/linux-auto-setup.sh`: почти автоматический Linux bootstrap для обычного пользователя.
- Добавлен `scripts/linux-smoke.sh`: короткий wrapper для host-check.
- Добавлен `docs/linux-verification-checklist.md`: базовый Linux checklist для maintainers.
- Добавлены npm scripts:
  - `verify:linux:auto`
  - `verify:linux:auto:host`
  - `verify:linux:auto:full`
  - `verify:linux:auto:release`
  - `setup:linux:auto`
- Исправлен Linux auto-launch в `crates/compat/auto-launch/src/lib.rs`:
  - `systemctl --user daemon-reload`
  - `systemctl --user enable mcpace.service`
  - `systemctl --user is-enabled --quiet mcpace.service`
  - `systemctl --user disable mcpace.service`
  - `NoNewPrivileges=true`
  - `UMask=077`
  - `Restart=on-failure`
  - `WantedBy=default.target`
  - systemd-safe escaping для `$` и `%`.
- В `scripts/linux-auto-setup.sh` добавлены:
  - XDG-style config/state creation;
  - local-only host `127.0.0.1`;
  - health check `/healthz`;
  - проверка реального systemd enablement;
  - fallback для минимальных дистрибутивов без GNU `realpath -m`.
- Добавлен contract-test для Linux autoflow.
- Docker npm install proof больше не hardcode-ит только `linux-x64-gnu`; он учитывает target package.
- Release hygiene усилен: archive script исключает `.claude`, `.codex`, `.omc`, `%SystemDrive%`.

## Что проверено в контейнере

Прошло:

```text
node -c scripts/linux-auto-check.mjs                 PASS
bash -n scripts/linux-auto-setup.sh                  PASS
node --test tests/node/linux-auto-check-contract.test.js  PASS, 7/7
npm run lint:npm -- --json                           PASS, 101/101 files
npm run test:repo:smoke                              PASS, 7/39 selected files
npm run verify:secrets -- --json                     PASS, 0 findings
npm run verify:linux:auto:host                       exit 0, status warn
```

`verify:linux:auto:host` дал `warn`, не `fail`, потому что в этом контейнере нет полноценного release окружения: нет `cargo`, `rustc`, `docker`, user systemd manager и готового `mcpace` binary. Обязательные source/package/security checks прошли; runtime/autostart smoke на реальной Linux-сессии нужно прогнать на машине с binary и `systemd --user`.

## Что остаётся обязательным на реальной Linux-машине

```bash
cargo build --release
scripts/linux-auto-setup.sh --root /tmp/mcpace-user-test --bin ./target/release/mcpace --skip-client-install
systemctl --user is-enabled --quiet mcpace.service
systemctl --user restart mcpace.service
curl -fsS http://127.0.0.1:39022/healthz
npm run verify:linux:auto:full
```

Для boot-before-login отдельно:

```bash
loginctl enable-linger "$USER"
```

Это не нужно всем пользователям; это нужно только когда MCPace должен работать после logout или до интерактивного login.

## Найденные риски, которые нельзя замалчивать

- “Все Linux-дистрибутивы” пока нельзя заявлять: Alpine/musl должен быть отдельным target/proof.
- WSL/containers часто не имеют обычного user systemd manager; там autostart должен быть degraded/warn, не fake-pass.
- `npx` upstream на Linux ломаются так же, как на Windows, если не проброшены npm/proxy/cert env var names.
- `0.0.0.0` нельзя считать безопасным local mode без auth.
- Release/source archive не должен включать локальное machine-state: `.claude`, `.codex`, `.omc`, `%SystemDrive%`, screenshots.

## Как применить

```bash
git apply mcpace_linux_autoflow_hardening_v3_safe.patch
npm run lint:npm
node --test tests/node/linux-auto-check-contract.test.js
npm run verify:linux:auto:host
```

Потом на реальной Linux-машине с Rust/Docker/systemd:

```bash
cargo build --release
npm run verify:linux:auto:full
scripts/linux-auto-setup.sh --root /tmp/mcpace-user-test --bin ./target/release/mcpace --skip-client-install
```
