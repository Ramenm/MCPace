#!/usr/bin/env bash
set -euo pipefail

: "${MCPACE_RUST_TARGET:?MCPACE_RUST_TARGET is required}"
: "${MCPACE_BINARY_NAME:=mcpace}"
: "${MCPACE_GLIBC_BASELINE_IMAGE:=ubuntu:22.04}"
: "${MCPACE_GLIBC_BASELINE_CHECKS:=build}"
: "${MCPACE_RUST_TOOLCHAIN:=1.95.0}"

case "$MCPACE_GLIBC_BASELINE_CHECKS" in
  build|full) ;;
  *)
    echo "MCPACE_GLIBC_BASELINE_CHECKS must be 'build' or 'full', got '$MCPACE_GLIBC_BASELINE_CHECKS'" >&2
    exit 2
    ;;
esac

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required to build Linux glibc baseline artifacts" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
host_uid="$(id -u)"
host_gid="$(id -g)"

mkdir -p "$repo_root/target" "$repo_root/.cargo-baseline" "$repo_root/.rustup-baseline"

docker run --rm   -e "CARGO_HOME=/work/.cargo-baseline"   -e "RUSTUP_HOME=/work/.rustup-baseline"   -e "MCPACE_RUST_TARGET=$MCPACE_RUST_TARGET"   -e "MCPACE_BINARY_NAME=$MCPACE_BINARY_NAME"   -e "MCPACE_GLIBC_BASELINE_CHECKS=$MCPACE_GLIBC_BASELINE_CHECKS"   -e "MCPACE_RUST_TOOLCHAIN=$MCPACE_RUST_TOOLCHAIN"   -e "HOST_UID=$host_uid"   -e "HOST_GID=$host_gid"   -v "$repo_root:/work"   -w /work   "$MCPACE_GLIBC_BASELINE_IMAGE"   bash -lc '
    set -euo pipefail
    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y --no-install-recommends ca-certificates curl build-essential pkg-config nodejs
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$MCPACE_RUST_TOOLCHAIN" --component rustfmt,clippy
    . "$CARGO_HOME/env"
    rustup target add "$MCPACE_RUST_TARGET"
    if [ "$MCPACE_GLIBC_BASELINE_CHECKS" = "full" ]; then
      cargo fmt --check
      cargo generate-lockfile --locked
      cargo clippy --locked --all-targets --target "$MCPACE_RUST_TARGET" -- -D warnings
      cargo test --locked --target "$MCPACE_RUST_TARGET" -- --test-threads=1
    fi
    cargo build --release --locked --target "$MCPACE_RUST_TARGET" --bins
    test -f "target/$MCPACE_RUST_TARGET/release/$MCPACE_BINARY_NAME"
    chown -R "$HOST_UID:$HOST_GID" target .cargo-baseline .rustup-baseline
  '
