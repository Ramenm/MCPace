# API Contract (project-native Markdown)

## Какой формат принят в проекте

Факт: в репозитории не найдено `openapi.*`, `swagger.*`, `schema.graphql`, `.proto`, `RAML` или другой формальной API-спеки.

Вывод: для этого проекта фактический формат спецификации — Markdown рядом с кодом и runtime-скриптами. Source template `mcp_settings.json` не должен содержать runtime state; effective settings живут только в generated runtime paths.

## Scope

Этот контракт фиксирует именно тот HTTP/MCP surface, который реально использует launcher:

- readiness и REST API `agent-browser-protocol`;
- health/status и MCP endpoints `MCPace`;
- optional Windows-only bridge `windows-mcp`.

Модель клиента для этого проекта:

- публичный MCP endpoint только один: `MCPace`;
- host-only bridges (`browser`, `windows-mcp`) считаются внутренним launcher/runtime слоем и не являются основным client-facing endpoint.

## Допущения и границы

Подтверждено кодом launcher-а:

- какие пути вызываются;
- какие методы используются;
- какие поля ответов реально читает проект.

НЕ ПОДТВЕРЖДЕНО автоматически в этом репозитории:

- полный upstream schema каждого ответа;
- полный список кодов ошибок каждого upstream endpoint;
- стабильность неиспользуемых полей.

Если upstream вернёт дополнительные поля — это совместимо. Если изменятся поля, которые launcher читает, это breaking change.

---

## 1. MCPace

### Base URL

`http://127.0.0.1:{hubPort}`

Где `{hubPort}` задаётся в `mcpace.config.json`.

### Authentication

Для `GET /api/servers` и для всех `POST /mcp*` launcher использует:

```http
Authorization: Bearer <token>
```

Токен берётся из `mcp_settings.json -> bearerKeys[]` после runtime auth resolution. В source template он должен приходить из `${MCPACE_BEARER_TOKEN}`, а фактическое значение может быть взято либо из env override, либо из generated local auth state под ignored runtime paths.

### 1.1 GET /health

Назначение: liveness/readiness probe для launcher-а.

Auth: не используется текущим launcher-ом.

#### Expected response fields consumed by project

| Field | Type | Required | Notes |
|---|---:|---:|---|
| `status` | string | yes | launcher ожидает минимум `healthy` / другое значение |
| `message` | string | no | выводится как диагностическое сообщение |
| `servers.total` | integer | no | общее число зарегистрированных серверов |
| `servers.connected` | integer | no | число подключённых серверов |
| `servers.disconnected` | integer | no | число отключённых серверов |

#### Example response

```json
{
  "status": "healthy",
  "message": "MCPace is healthy",
  "servers": {
    "total": 4,
    "connected": 4,
    "disconnected": 0
  }
}
```

#### Statuses

- `200` — probe response received
- `5xx` or network error — launcher treats hub as offline

#### Breaking change markers

- **BREAKING**: убрать `status`
- **BREAKING**: изменить `servers.total|connected|disconnected` на несовместимую форму
- **BREAKING**: потребовать auth, не обновив launcher

### 1.2 GET /api/servers

Назначение: список серверов и их runtime state для `check.ps1`.

Auth: required.

#### Response fields consumed by project

| Field | Type | Required | Notes |
|---|---:|---:|---|
| `data` | array | yes | launcher ожидает массив |
| `data[].name` | string | yes | имя сервера |
| `data[].status` | string | yes | `connected`, `disconnected`, `disabled`, `connecting`, etc. |
| `data[].enabled` | boolean | no | используется для отличия disabled от offline |
| `data[].error` | string | no | диагностическая причина |

#### Example request

```bash
curl -s \
  -H "Authorization: Bearer $MCPACE_BEARER_TOKEN" \
  http://127.0.0.1:12223/api/servers
```

#### Example response

```json
{
  "data": [
    {
      "name": "browser",
      "status": "connected",
      "enabled": true,
      "error": ""
    },
    {
      "name": "windows-mcp",
      "status": "disabled",
      "enabled": false,
      "error": ""
    }
  ]
}
```

#### Statuses

- `200` — data envelope returned
- `401` — missing or invalid bearer token
- `403` — token exists but not authorized
- `5xx` — launcher prints no server list and treats data as unavailable

#### Breaking change markers

- **BREAKING**: вернуть массив без envelope `data`
- **BREAKING**: переименовать `status` или `enabled`

### 1.3 POST /mcp

Назначение: unified MCP endpoint для всех серверов.

Auth: required by default.

Transport: JSON-RPC over Streamable HTTP.

Launcher smoke test и compatibility gate подтверждают минимум такой обмен:

1. `initialize`
2. `notifications/initialized`
3. `tools/list`
4. `resources/list`
5. `resources/templates/list`
6. повторный request в той же session после короткого idle
7. новый `initialize` после reconnect

#### Example initialize request

```http
POST /mcp HTTP/1.1
Authorization: Bearer <token>
Content-Type: application/json
Accept: application/json, text/event-stream

{
  "jsonrpc": "2.0",
  "id": 0,
  "method": "initialize",
  "params": {
    "protocolVersion": "2025-06-18",
    "capabilities": {},
    "clientInfo": {
      "name": "mcpace-smoke",
      "version": "1.0.0"
    }
  }
}
```

#### Example initialize response expectations

| Header / field | Required | Notes |
|---|---:|---|
| `mcp-session-id` response header | yes | launcher requires it for follow-up calls |
| `result.serverInfo.version` | no | used only for diagnostics |

#### Example tools/list request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/list",
  "params": {}
}
```

#### Example tools/list response expectation

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": []
  }
}
```

#### Statuses

- `200` — request accepted
- `400` — invalid JSON-RPC payload
- `401` / `403` — auth failure
- `5xx` — upstream error inside hub or server chain

#### Breaking change markers

- **BREAKING**: перестать возвращать `mcp-session-id`
- **BREAKING**: несовместимо изменить `tools/list` result shape

### 1.4 POST /mcp/{group}
### 1.5 POST /mcp/{server}

Назначение: scoped MCP endpoints.

Project use today: не вызываются launcher-ом напрямую, но они являются частью поддерживаемого public surface проекта и должны оставаться совместимыми с upstream MCPace routing.

Совместимые требования те же, что и для `POST /mcp`.

---

## 2. ABP (`agent-browser-protocol`)

### Base URL

`http://127.0.0.1:{abpPort}`

### Auth

Launcher auth не использует. Подразумевается localhost-only exposure.

### 2.1 GET /api/v1/tabs

Назначение: основной readiness probe.

#### Example request

```bash
curl -s http://127.0.0.1:39022/api/v1/tabs
```

#### Contract used by project

Launcher не зависит от конкретной формы тела ответа. Для readiness достаточно любого успешного HTTP response.

#### Statuses

- `200` — ABP reachable
- non-`200` / network error — launcher проверяет fallback endpoint

#### Breaking change markers

- **BREAKING**: удалить endpoint без альтернативы

### 2.2 GET /api/v1/browser/status

Назначение: fallback readiness probe.

#### Response fields consumed by project

| Field | Type | Required | Notes |
|---|---:|---:|---|
| `data.ready` | boolean | preferred | current preferred shape |
| `ready` | boolean | fallback | compatibility fallback in launcher |

#### Example response

```json
{
  "data": {
    "ready": true
  }
}
```

#### Breaking change markers

- **BREAKING**: убрать и `data.ready`, и `ready`

### 2.3 POST /mcp

Назначение: upstream browser MCP endpoint, к которому MCPace подключается через `mcp-remote`.

Project use today: косвенный, через `browser` server в `mcp_settings.json`.

Совместимые требования:

- streamable HTTP transport
- стабильный MCP JSON-RPC handshake

---

## 3. Optional Windows-only bridge

### Base URL

`http://127.0.0.1:8233`

### 3.1 POST /mcp

Назначение: optional host bridge для `windows-mcp`.

Статус:

- template-disabled in `mcp_settings.json`
- auto-enabled only on supported Windows host after launcher preflight
- auto-disabled in effective settings when platform or host prerequisites do not match

Breaking:

- **BREAKING** только для Windows-пользователей, которые вручную включили `windows-mcp`

---

## Что отсутствует или расходится с реализацией

1. Формальная machine-readable API-спека отсутствует.
2. Launcher зависит от конкретных минимальных полей upstream ответов, но эти зависимости не были раньше зафиксированы в репозитории.
3. В старой документации были несогласованные значения портов и launcher paths.
4. `.vscode/mcp.json` раньше дублировал прямые подключения к множеству серверов и расходился с фактической runtime-архитектурой “один endpoint через MCPace”.

## Breaking changes и рекомендации по versioning

### Breaking changes, уже внесённые в проектный слой

- `.vscode/mcp.json` упрощён до одного сервера `mcpace`.
- `windows-mcp` выключен по умолчанию.
- чувствительные значения в `mcp_settings.json` заменены env placeholders.

### Рекомендации

- Изменения локальных launcher contracts без смены upstream API — `minor`.
- Изменения default server set или client profile generation — `minor`, с явным changelog.
- Любое несовместимое изменение полей `/health`, `/api/servers`, ABP readiness probes или MCP session headers — `major`.

## Как проверить актуальность спецификации

1. Сверить фактические вызовы в `lib/runtime.ps1`, `check.ps1`, `smoke-test.ps1`.
2. Выполнить:

```bash
pwsh ./check.ps1
pwsh ./smoke-test.ps1
```

3. Ручная проверка HTTP:

```bash
curl -s http://127.0.0.1:39022/api/v1/tabs
curl -s http://127.0.0.1:12223/health
curl -s -H "Authorization: Bearer $MCPACE_BEARER_TOKEN" http://127.0.0.1:12223/api/servers
```

4. Если ответная форма изменилась — сначала обновить этот документ, затем launcher code.
