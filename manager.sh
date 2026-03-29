#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"

if ! command -v pwsh >/dev/null 2>&1; then
  echo "PowerShell 7 (pwsh) is required." >&2
  exit 1
fi

command_name="${1:-help}"
if [ "$#" -gt 0 ]; then
  shift
fi

case "$command_name" in
  help|-h|--help)
    cat <<'EOF'
Usage:
  sh ./manager.sh <command> [PowerShell script args...]

Commands:
  install         -> install.ps1
  boot            -> boot.ps1
  check           -> check.ps1
  smoke           -> smoke-test.ps1
  readiness       -> validate-readiness.ps1
  verify          -> verify-manager.ps1
  repair          -> repair.ps1
  build-release   -> build-release.ps1
  auth            -> auth.ps1
  autostart       -> autostart.ps1
  backup          -> backup.ps1
  rotate-logs     -> rotate-logs.ps1
  setup-clients   -> setup-mcp-clients.ps1
EOF
    exit 0
    ;;
  install) script_name="install.ps1" ;;
  boot) script_name="boot.ps1" ;;
  check) script_name="check.ps1" ;;
  smoke) script_name="smoke-test.ps1" ;;
  readiness) script_name="validate-readiness.ps1" ;;
  verify) script_name="verify-manager.ps1" ;;
  repair) script_name="repair.ps1" ;;
  build-release) script_name="build-release.ps1" ;;
  auth) script_name="auth.ps1" ;;
  autostart) script_name="autostart.ps1" ;;
  backup) script_name="backup.ps1" ;;
  rotate-logs) script_name="rotate-logs.ps1" ;;
  setup-clients) script_name="setup-mcp-clients.ps1" ;;
  *)
    echo "Unknown command: $command_name" >&2
    echo "Run 'sh ./manager.sh help' for usage." >&2
    exit 1
    ;;
esac

exec pwsh -NoProfile -File "$script_dir/$script_name" "$@"
