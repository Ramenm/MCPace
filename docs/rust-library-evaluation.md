# Rust Library Evaluation

| Concern | Choice | Status | Why |
|---|---|---|---|
| CLI parsing | `clap` | selected next | good grouped command model and help generation |
| MCP protocol layer | `rmcp` | selected next | official Rust MCP SDK |
| Async runtime | `tokio` | selected next | needed for real hub runtime and transports |
| JSON/config | `serde`, `serde_json` | selected now | baseline for manifests, reports, config |
| Schema generation | `schemars` | selected next | keep Rust types and JSON Schema aligned |
| Schema validation | `jsonschema` | selected next | validate config/examples in code and tests |
| Errors | `anyhow`, `thiserror` | selected next | clean top-level context plus typed domain errors |
| Local state | `rusqlite` | selected next | lightweight durable local metadata store |
| Logs | `tracing`, `tracing-subscriber`, `tracing-appender` | selected next | structured diagnostics and rolling logs |
