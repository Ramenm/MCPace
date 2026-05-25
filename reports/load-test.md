# MCPace load-test report

No fresh load test result is bundled with this source-only archive.

The release manifest requires this file so source packaging can stay reproducible, but a real runtime load proof must be regenerated on a Rust-capable host after building the native binary:

```bash
cargo build --release
npm run load:local -- --binary ./target/release/mcpace --duration-ms 5000 --concurrency 64
# or set MCPACE_BINARY_PATH=./target/release/mcpace and run npm run load:local
```

Treat this archive as not load-tested until that command passes and this report is replaced with measured p50/p95/p99 latency, error-count, platform, CPU, memory, and binary-version data.
