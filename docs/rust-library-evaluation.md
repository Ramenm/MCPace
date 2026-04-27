# Rust Library Evaluation

| Concern | Choice | Status | Why |
|---|---|---|---|
| User autostart | `auto-launch` | selected now | replaces handwritten Windows/macOS/Linux startup registration while staying user-level |
| Executable discovery | `which` | selected now | replaces manual `PATH`/`PATHEXT` scanning in host diagnostics |
| CLI parsing | `clap` | selected next | good grouped command model and help generation |
| MCP protocol layer | `rmcp` | selected next | official Rust MCP SDK |
| Async runtime | `tokio` | selected next | needed for real hub runtime and transports |
| JSON/config | `serde_json` | selected now | deletes MCPace's handwritten JSON parser/formatter while preserving the existing `JsonValue` compatibility wrapper |
| TOML editing | `toml_edit` | evaluate next | can replace string-built managed TOML blocks while preserving existing user config better than raw serialization |
| YAML editing | `serde_yml` / `yaml-rust2` | evaluate next | can reduce bespoke YAML section scanning, but must preserve user comments and managed-block idempotency |
| HTTP client probes | `ureq` | evaluate later | can replace setup's raw `TcpStream` HTTP requests; defer because current probes are localhost-only and adding TLS-capable HTTP dependencies is heavier |
| HTTP server parsing | `tiny_http` / `axum` | evaluate later | dashboard/serve have handwritten HTTP framing; migrate only with endpoint regression tests because this is runtime-critical |
| Schema generation | `schemars` | selected next | keep Rust types and JSON Schema aligned |
| Schema validation | `jsonschema` | selected next | validate config/examples in code and tests |
| Errors | `anyhow`, `thiserror` | selected next | clean top-level context plus typed domain errors |
| Local state | `rusqlite` | selected next | lightweight durable local metadata store |
| Logs | `tracing`, `tracing-subscriber`, `tracing-appender` | selected next | structured diagnostics and rolling logs |
