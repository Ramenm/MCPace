# RFC — Zero-Touch Multi-Project Routing for MCPace

## Статус

Draft / design note. Это не принятое архитектурное решение и не финальный контракт.

## Зачем нужен этот документ

Сейчас `MCPace` уже умеет multi-root v1:

- есть `manager root`;
- есть `primary workspace`;
- есть `extra workspaces`;
- клиент по-прежнему подключается только к одному endpoint `mcpace`.

Но этого недостаточно для сценария, где один пользователь одновременно ведёт 10-20 проектов и не хочет вручную выбирать проект, делать `bind` или поддерживать отдельные endpoint'ы на каждый репозиторий.

Цель этого документа: зафиксировать желаемое направление развития без изменения текущего runtime.

## Проблема

Текущая модель закрывает только первую часть задачи:

- `filesystem` умеет видеть несколько workspace roots;
- shared/global MCP-серверы продолжают работать через единый hub;
- `primary` и `extras` уже отделены от manager data.

Но остаются ограничения:

- `multi-root v1` умеет много проектов, но `primary` один;
- `serena` и `lean-ctx` остаются фактически single-project-by-default;
- клиенты не должны знать о 20 endpoint'ах;
- пользователь не должен делать ручной `bind` или переключение на каждый запрос.

## Цель

Целевая модель:

- все клиенты знают только один endpoint `MCPace`;
- пользователь не создаёт проекты вручную в ежедневной работе;
- hub сам обнаруживает проекты;
- hub сам держит project registry;
- hub сам маршрутизирует запросы в нужный проект;
- project-local state не смешивается между проектами.

Это **не** цель “магия без источника данных”.

Правильная цель: **zero-touch для пользователя в ежедневной работе**, а не zero-knowledge для сервера.

## Не-цели

- Не делать client-specific логику для Codex, Claude, Cursor или другого MCP-клиента.
- Не требовать ручного выбора проекта на каждый запрос.
- Не создавать по умолчанию множество endpoint'ов, по одному на проект.
- Не считать “сканировать весь диск без ограничений” нормальной архитектурой.
- Не менять клиентский контракт `mcpace` только ради multi-project UX.

## Основная идея

### 1. Один endpoint для всех клиентов

Клиенты продолжают работать только с одним endpoint:

- `mcpace`

Вся логика выбора проекта должна жить внутри `MCPace`, а не в клиентах.

### 2. Auto-discovery вместо ручного bind

Пользователь не должен заводить 20 проектов вручную.

Вместо этого hub получает небольшой набор discovery roots и сам ищет внутри них проекты по эвристикам:

- `.git`
- `package.json`
- `pyproject.toml`
- `Cargo.toml`
- `go.mod`
- `.sln`
- другие project markers по мере необходимости

Дальше hub строит внутренний registry проектов.

### 3. Автоматический выбор проекта

Hub должен выбирать проект в таком порядке:

1. По явному пути, если путь уже присутствует в запросе.
2. По session context, если в текущей сессии уже шла работа с конкретным проектом.
3. По recent usage / last-used context, если неоднозначности нет.
4. Запросить уточнение только тогда, когда выбрать проект безопасно нельзя.

### 4. Project-local instances для stateful servers

Не все MCP-серверы одинаково подходят для multi-project режима.

Серверы, которые держат cwd/state/index/cache, нельзя честно шарить одним экземпляром на 20 проектов.

Поэтому для таких серверов нужна модель project-local instances:

- `serena`
- `lean-ctx`
- потенциально `git`, если он должен работать как project-aware tool, а не только как manager-side utility

### 5. Shared/global servers остаются общими

Некоторые серверы не должны размножаться на каждый проект.

Shared/global логика подходит для:

- `filesystem`
- `demo-server`
- `windows-mcp`
- `exa`
- `fetch`
- `sequential-thinking`

## Почему не bind

Идея явного `bind` или client-visible project switch выглядит плохо по нескольким причинам:

- это плохой UX;
- это видимый пользователю state, который легко забыть или перепутать;
- это плохо переносится между разными MCP-клиентами;
- это превращает архитектурную задачу hub-а в проблему каждого отдельного клиента.

Идеальный режим: пользователь просто работает, а hub сам понимает, какой проект сейчас релевантен.

## Почему это вообще возможно

Это возможно, если вся автоматика живёт внутри `MCPace`.

Для этого не нужен hardcode в клиентах.

Нужно только:

- server-side project discovery;
- project registry;
- session-aware routing;
- project-local instances для stateful tools.

Иными словами:

- один endpoint у клиента сохранить можно;
- zero-touch UX для пользователя сделать можно;
- полностью без server-side discovery source сделать это нельзя.

## Ограничения

- Без discovery source невозможно угадать все проекты.
- При неоднозначности hub не должен угадывать вслепую.
- `workspace-scoped` серверы нельзя честно шарить одним экземпляром на 20 проектов.
- Сканировать весь диск без ограничений нельзя считать безопасной или дешёвой стратегией.
- Автоматический выбор проекта должен быть “sticky, but reversible”: hub может закреплять текущий проект за сессией, но не должен делать это необратимо и глобально для всех клиентов.

## Manager-side сущности

### DiscoveryRoot

Описывает корень, внутри которого hub ищет проекты.

Intended shape:

- `path`
- `scanDepth`
- `enabled`
- `includePatterns`
- `excludePatterns`
- `trustLevel`

### ProjectRegistryEntry

Описывает один найденный проект.

Intended shape:

- `projectId`
- `name`
- `hostPath`
- `containerPath`
- `markers`
- `detectedType`
- `access`
- `lastSeenAt`
- `lastUsedAt`
- `state`

### SessionActiveProject

Описывает текущий активный проект в рамках конкретной клиентской сессии.

Intended shape:

- `sessionId`
- `projectId`
- `selectedBy`
- `selectedAt`
- `confidence`

### ServerCapabilityClass

Классификация серверов по модели маршрутизации и жизненного цикла.

Поддерживаемые классы:

- `host-global`
- `remote-global`
- `path-scoped`
- `workspace-scoped`

## Intended config shape

Финальный wire format здесь не фиксируется. Ниже только intended shape.

### Discovery config

- discovery roots
- scan policy
- include/exclude rules
- cache invalidation policy

### Project registry persistence

- persistent cache в manager data
- timestamps last-seen / last-used
- stale-entry cleanup

### Routing policy

- path-first routing
- session affinity
- ambiguity handling
- optional explicit override

### Per-server capability class

Каждый сервер должен быть явно помечен одной capability-class:

- `host-global`
- `remote-global`
- `path-scoped`
- `workspace-scoped`

## Клиентский контракт

Клиентский контракт должен остаться прежним:

- один `mcpace` endpoint;
- никакого project bind в клиентах;
- никакого множества endpoint'ов по умолчанию;
- никакой client-specific логики под отдельный редактор или агент.

## Сценарии, которые архитектура должна закрыть

### Базовые

- один пользователь, 20 проектов, один MCP endpoint;
- два разных клиента одновременно работают с разными проектами;
- путь `/workspaces/<name>/...` сразу направляет запрос в нужный проект;
- pathless запрос идёт в session active project;
- неоднозначный запрос не угадывается вслепую;
- `ro` проекты не становятся writable;
- `serena` и `lean-ctx` не смешивают state между проектами.

### Проверки будущей реализации

- Windows host
- Linux/macOS
- cold-start большого registry
- lazy startup project-local instances
- eviction / idle timeout неактивных instances
- recovery после удаления или перемещения проекта на диске

## Отдельная заметка про sequential-thinking

Текущий launcher не меняет `sequential-thinking`.

Факты:

- в шаблоне он подключён как обычный stdio server;
- в effective settings он таким же и остаётся;
- workspace-aware launcher не добавляет ему `workspaceBinding`;
- current upstream server реально отдаёт один tool.

Следовательно:

- один tool у `sequential-thinking` — это не следствие workspace-aware рефактора;
- если раньше tools было больше, значит использовался другой upstream package, другая версия или другой сервер.

## Практический вывод

Zero-touch multi-project режим возможен.

Но он должен строиться не на:

- ручном `bind`;
- множестве клиентских endpoint'ов;
- hardcode внутри конкретных клиентов.

Он должен строиться на:

- auto-discovery;
- project registry;
- session-aware routing;
- project-local instances для stateful tools;
- одном общем `MCPace` endpoint для всех клиентов.
