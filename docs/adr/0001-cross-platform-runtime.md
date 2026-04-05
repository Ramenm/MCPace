# ADR 0001 — Cross-platform runtime normalization without full rewrite

## Допущения

- Проект остаётся локальным single-user launcher, а не multi-tenant сервисом.
- Не все MCP-серверы обязаны работать на всех платформах.
- Пользовательский сценарий с единым MCP endpoint важнее, чем прямое подключение клиента к каждому отдельному серверу.

## Контекст

Репозиторий — это PowerShell-based launcher для `agent-browser-protocol` на хосте и `MCPace` в Docker. До изменения проект был Windows-first: в коде и документации были жёсткие предположения о `powershell`, `npx.cmd`, `Get-NetTCPConnection`, `schtasks.exe`, `.cmd` launchers и Windows-only сервере `windows-mcp`.

## Проблема / цель

Нужно упростить проект, уменьшить дублирование, убрать секреты из репозитория и сделать базовый сценарий запуска сопровождаемым на Windows, Linux и macOS без полного переписывания launcher-а.

## Ограничения и не-цели

- Не делать полный rewrite на Node.js или Python в этом изменении.
- Не гарантировать переносимость каждого дополнительного MCP-сервера.
- Не менять upstream API MCPace и ABP.
- Не добавлять сложную оркестрацию уровня Kubernetes/Compose-first без подтверждённой необходимости.

## Рассмотренные варианты

1. Оставить Windows-first сценарий и только обновить README.
2. Полностью переписать launcher на Node.js.
3. Эволюционно перевести существующий PowerShell launcher на cross-platform runtime abstractions.

## Выбранное решение

Выбран вариант 3.

Что принято:

- сохраняем текущий PowerShell launcher как основной entrypoint;
- рекомендуем `pwsh` как основной shell для Windows/Linux/macOS;
- вводим platform-aware helpers в `lib/runtime.ps1`;
- генерируем effective settings с env expansion и platform-specific disablement;
- оставляем включённым безопасный базовый набор MCP-серверов и только те optional server'ы, у которых есть zero-touch managed install/runtime;
- убираем секреты из репозитория, заменяя их env placeholders;
- минимизируем клиентский профиль `.vscode/mcp.json` до одного endpoint `mcpace`.

## Последствия / риски

Плюсы:

- меньше drift между runtime и docs;
- меньше зависимостей по умолчанию;
- базовый запуск стал реалистично переносимым;
- optional servers без secrets/OAuth и с managed install не ломают запуск сразу после распаковки.

Минусы / риски:

- runtime всё ещё зависит от Docker и Node.js;
- autostart по-прежнему Windows-only;
- интерактивный dashboard не был полноценно прогнан в этой среде;
- cross-platform runtime smoke всё ещё не доказан на всех целевых платформах.

## План внедрения

1. Поддерживать `pwsh` как основной способ запуска.
2. Держать `lib/runtime.ps1` единой точкой platform abstractions.
3. Любые новые MCP-серверы по умолчанию добавлять disabled, если им нужны секреты, OAuth или нестандартные бинарники без managed install/preflight.
4. Поддерживать проектную API-спеку в `docs/api-contract.md`.
5. При появлении второго контейнера или сложной сетевой схемы пересмотреть переход на `docker compose`.

## Открытые вопросы

- Нужен ли отдельный `docker-compose.yml` как non-interactive Plan B?
- Нужно ли дополнительно ограничить локальный auth bootstrap для shared-host сценариев?
- Нужна ли CI-проверка PowerShell на Windows + Linux runner, чтобы убрать статус `НЕ ПОДТВЕРЖДЕНО` для runtime?
