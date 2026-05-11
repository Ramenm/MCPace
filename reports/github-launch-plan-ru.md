# MCPace: план доведения до сильного GitHub-проекта

## Цель

Сделать MCPace публичным проектом, который выглядит не как сырой эксперимент, а как честный, проверяемый, полезный developer tool:

- понятно, что он делает;
- понятно, где границы;
- понятно, как запустить;
- понятно, как проверить;
- понятно, как контрибьютить;
- понятно, почему ему можно доверять.

## Позиционирование

Не запускать с обещанием “универсальный MCP runtime для всего”. Сильнее и честнее:

> MCPace — Rust-first локальный MCP hub: один локальный MCP endpoint для многих клиентов, BYO upstream MCP servers, безопасные дефолты и proof-driven diagnostics.

Это понятная боль: MCP servers часто приходится настраивать в каждом клиенте отдельно. MCPace должен стать локальным, проверяемым, аккуратным маршрутизатором и control-plane для этого процесса.

## Что сделано в этой рабочей копии

- Добавлен stateful in-process Streamable HTTP session store для `/mcp`: `initialize` создаёт/заменяет session record, последующие stateful requests требуют известный `Mcp-Session-Id`, unknown/expired/closed sessions получают `404`, missing/invalid/mismatch — `400`, `DELETE /mcp` закрывает сессию.
- `runtime-trace-harness` держит target-aware binary discovery, включая `packages/npm/cli/vendor/<target>/mcpace`.
- `product-practice-harness` стал строже: runtime/release claims блокируются, если proof report старый, от другой версии/хоста или без текущего binary proof. Default freshness window — 6 часов.
- Добавлен GitHub launch kit: `ROADMAP.md`, `CHANGELOG.md`, `SUPPORT.md`, `CODE_OF_CONDUCT.md`, усиленные `CONTRIBUTING.md`/`SECURITY.md`, issue templates, PR template, release category config.
- Добавлены security/repo workflows: dependency review, CodeQL, OpenSSF Scorecard, Dependabot, GitHub readiness/health audit.
- README и docs обновлены так, чтобы не обещать HTTP upstream fan-out, public relay или published native install до proof artifacts.

## P0 перед публичной beta-заявкой

1. Прогнать свежий Rust proof в среде с доступом к crates.io/cache:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo build --release --locked
```

2. Сгенерировать свежие reports в той же release/CI lane:

```bash
npm run verify:boot
npm run verify:install-readiness
npm run verify:runtime-trace
npm run verify:product-practice
```

3. Получить real-client proof:

```text
real MCP client -> MCPace /mcp -> initialize -> tools/list -> tools/call -> upstream result
```

4. Реализовать HTTP/Streamable HTTP upstream fan-out отдельно от stdio:

- connector interface для stdio/http;
- SSRF/scheme/host/port checks;
- auth/header isolation;
- timeouts/cancellation;
- tiny HTTP MCP fixture tests.

5. Довести published install:

- stage/verify binaries for supported targets;
- platform npm packages;
- checksums/attestations;
- npm Trusted Publishing/OIDC;
- release dry-run green.

## Что делает проект “звёздочным”

Звёзды обычно дают не за “много кода”, а за понятность и доверие:

- короткий tagline;
- quickstart, который реально работает;
- честный status table;
- видимый security posture;
- demo/GIF после runtime proof;
- хорошая диагностика вместо “читай исходники”;
- dry-run/diff/backup/restore для config mutation;
- roadmap без overclaim;
- регулярные маленькие релизы;
- issue templates, где контрибьютор понимает, какую proof нужно приложить.

## Текущий честный статус

Сейчас MCPace можно сильнее позиционировать как source/control-plane ready проект с connectable runtime preview. До runtime beta ещё нужны fresh Rust proof, native binary proof, real-client trace и HTTP upstream fan-out.
