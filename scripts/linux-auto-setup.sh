#!/usr/bin/env bash
set -Eeuo pipefail

DEFAULT_PORT="39022"
ROOT="${MCPACE_ROOT:-}"
BIN="${MCPACE_BIN:-${MCPACE_BINARY_PATH:-}}"
PORT="${MCPACE_SERVE_PORT:-$DEFAULT_PORT}"
INSTALL_CLIENTS="0"
RUN_SMOKE="1"
SERVERS="${MCPACE_SERVERS:-}"

usage() {
  cat <<'USAGE'
Usage: scripts/linux-auto-setup.sh [options]

User-level Linux bootstrap for MCPace. It creates a safe XDG config root when
missing, initializes runtime state, starts the local endpoint, and keeps client
configs untouched unless --install-clients is passed.

Options:
  --root <path>          MCPace user/project root. Default: $MCPACE_ROOT or $XDG_CONFIG_HOME/mcpace
  --bin <path>           mcpace binary. Default: $MCPACE_BIN, $MCPACE_BINARY_PATH, PATH, or target/release/mcpace
  --port <n>             Local MCPace port. Default: 39022
  --install-clients      Allow setup to write supported local client config entries
  --skip-client-install  Keep client configs untouched. This is the default.
  --no-smoke             Create/init only; do not start setup smoke
  --servers a,b          After setup, test selected upstream MCP servers
  -h, --help             Show this help
USAGE
}

log() { printf '== %s ==\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) ROOT="${2:-}"; shift 2 ;;
    --bin) BIN="${2:-}"; shift 2 ;;
    --port) PORT="${2:-}"; shift 2 ;;
    --install-clients) INSTALL_CLIENTS="1"; shift ;;
    --skip-client-install) INSTALL_CLIENTS="0"; shift ;;
    --no-smoke) RUN_SMOKE="0"; shift ;;
    --servers) SERVERS="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unsupported argument: $1" ;;
  esac
done

[[ "$(uname -s)" == "Linux" ]] || fail "this bootstrap is Linux-only"
case "$PORT" in ''|*[!0-9]*) fail "--port must be numeric" ;; esac
(( PORT >= 1 && PORT <= 65535 )) || fail "--port must be between 1 and 65535"

if [[ -z "$ROOT" ]]; then
  ROOT="${XDG_CONFIG_HOME:-$HOME/.config}/mcpace"
fi
if command -v python3 >/dev/null 2>&1; then
  ROOT="$(python3 -c 'import os,sys; print(os.path.abspath(os.path.expanduser(sys.argv[1])))' "$ROOT")"
else
  ROOT="$(realpath -m "$ROOT")"
fi

if [[ -z "${MCPACE_STATE_ROOT:-}" ]]; then
  export MCPACE_STATE_ROOT="${XDG_STATE_HOME:-$HOME/.local/state}/mcpace"
fi

resolve_bin() {
  if [[ -n "$BIN" ]]; then printf '%s\n' "$BIN"; return; fi
  if command -v mcpace >/dev/null 2>&1; then command -v mcpace; return; fi
  if [[ -x ./target/release/mcpace ]]; then printf '%s\n' "./target/release/mcpace"; return; fi
  if [[ -x ./target/debug/mcpace ]]; then printf '%s\n' "./target/debug/mcpace"; return; fi
  return 1
}

BIN="$(resolve_bin)" || fail "mcpace binary not found; pass --bin /path/to/mcpace or install @mcpace/cli"
[[ -x "$BIN" ]] || fail "mcpace binary is not executable: $BIN"

mkdir -p "$ROOT" "$ROOT/mcp_settings.d" "$MCPACE_STATE_ROOT"
chmod 700 "$ROOT" "$ROOT/mcp_settings.d" "$MCPACE_STATE_ROOT" 2>/dev/null || true

CONFIG_PATH="$ROOT/mcpace.config.json"
SETTINGS_PATH="$ROOT/mcp_settings.json"
README_PATH="$ROOT/mcp_settings.d/README.md"

if [[ ! -f "$CONFIG_PATH" ]]; then
  log "Creating user-level mcpace.config.json at $CONFIG_PATH"
  VERSION="$($BIN version 2>/dev/null | head -n 1 || true)"
  [[ -n "$VERSION" ]] || VERSION="0.0.0"
  cat > "$CONFIG_PATH" <<CONFIG_EOF
{
  "name": "mcpace-user",
  "version": "$VERSION",
  "serve": {
    "host": "127.0.0.1",
    "port": $PORT,
    "mcpPath": "/mcp",
    "description": "Local-only endpoint created by scripts/linux-auto-setup.sh."
  },
  "mcpSettings": {
    "includePaths": [],
    "includeDirs": ["mcp_settings.d"],
    "description": "Keep secrets in environment variables and reference them through env_vars."
  },
  "servers": {},
  "client": { "keyName": "MCPace" }
}
CONFIG_EOF
  chmod 600 "$CONFIG_PATH" 2>/dev/null || true
else
  log "Keeping existing config: $CONFIG_PATH"
fi

if [[ ! -f "$SETTINGS_PATH" ]]; then
  log "Creating empty mcp_settings.json"
  printf '{\n  "mcpServers": {}\n}\n' > "$SETTINGS_PATH"
  chmod 600 "$SETTINGS_PATH" 2>/dev/null || true
fi

if [[ ! -f "$README_PATH" ]]; then
  cat > "$README_PATH" <<'README_EOF'
Drop additional MCP settings JSON files here. Later files override earlier ones.
Prefer env_vars over inline env values so secrets stay outside config files.
README_EOF
fi

log "Preflight"
if [[ -f scripts/linux-auto-check.mjs ]] && command -v node >/dev/null 2>&1; then
  node scripts/linux-auto-check.mjs --profile host --no-docker --root "$ROOT" --bin "$BIN" --create-dirs || true
else
  warn "scripts/linux-auto-check.mjs or node not available; skipping extended preflight"
fi

if [[ "$RUN_SMOKE" == "1" ]]; then
  SETUP_ARGS=(setup --json --root "$ROOT" --host 127.0.0.1 --port "$PORT")
  if [[ "$INSTALL_CLIENTS" != "1" ]]; then SETUP_ARGS+=(--skip-client-install); fi
  log "Run setup smoke"
  "$BIN" "${SETUP_ARGS[@]}"
else
  log "Skipping setup smoke by request"
fi

if [[ -n "$SERVERS" ]]; then
  IFS=',' read -r -a SERVER_LIST <<< "$SERVERS"
  for server in "${SERVER_LIST[@]}"; do
    server="${server//[[:space:]]/}"
    [[ -n "$server" ]] || continue
    log "Testing upstream server: $server"
    "$BIN" server test "$server" --refresh --timeout-ms 30000 --json --root "$ROOT" || true
  done
fi

cat <<DONE_EOF

MCPace Linux setup complete.
Root:       $ROOT
State root: $MCPACE_STATE_ROOT
Binary:     $BIN
Endpoint:   http://127.0.0.1:$PORT/mcp

Useful next checks:
  $BIN doctor --json --root "$ROOT"
  $BIN server list --json --root "$ROOT"
  $BIN serve stop --json --root "$ROOT"
DONE_EOF
