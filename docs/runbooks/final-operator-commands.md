# Final operator commands

## One-command source proof

```bash
npm run verify:ideal:source
```

## Full doctor only

```bash
npm run doctor:full
```

## Harden live config, dry run

```bash
node scripts/mcpace-config-hardener.mjs --config "$HOME/.mcpace/mcp_settings.d/restored-from-mcpace-history-72d64b0.json" --json
```

## Harden live config, apply

```bash
node scripts/mcpace-config-hardener.mjs --config "$HOME/.mcpace/mcp_settings.d/restored-from-mcpace-history-72d64b0.json" --apply --json
```

## MCP HTTP smoke

```bash
node scripts/mcp-http-smoke.mjs --url http://127.0.0.1:39022/mcp --json
```

## Linux near-zero-touch setup

```bash
scripts/linux-auto-setup.sh --root /tmp/mcpace-user-test --bin ./target/release/mcpace --skip-client-install
```
