# Supported clients

MCPace keeps one local MCP endpoint and patches client configs only when a supported local config or installed CLI is detected.

| Client mode | Behavior |
|---|---|
| `--client auto` | Default. Patch detected clients only. |
| `--client cursor-local` | Patch Cursor local config when present. |
| `--client all` | Patch every supported detected client. |
| `--client none` | Start MCPace without touching client config. |

The updater preserves unrelated server entries and skips MCPace self-references to avoid routing loops.
