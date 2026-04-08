# Technology / Approach Comparison

## 1) Источники

1. Microsoft Learn — PowerShell migration and install docs  
   https://learn.microsoft.com/en-us/powershell/scripting/whats-new/migrating-from-windows-powershell-51-to-powershell-7?view=powershell-7.6  
   Почему релевантно: официальный источник по side-by-side работе PowerShell 7 и Windows PowerShell 5.1, а также по supported platforms.

2. Microsoft Learn — Install PowerShell on Windows, Linux, and macOS  
   https://learn.microsoft.com/en-us/powershell/scripting/install/install-powershell?view=powershell-7.6  
   Почему релевантно: подтверждает, что `pwsh` — нормальная cross-platform база для launcher-а.

3. Docker Docs — Networking how-tos / host access  
   https://docs.docker.com/desktop/features/networking/networking-how-tos/  
   Почему релевантно: официальный способ доступа контейнера к сервису на host через `host.docker.internal`.

4. Docker Docs — `host-gateway` for `--add-host`  
   https://docs.docker.com/reference/cli/dockerd/  
   Почему релевантно: официальный Linux-совместимый способ подставить alias до host из контейнера.

5. MCPace docs / repo  
   https://docs.mcphub.app/  
   https://github.com/samanhappy/mcphub  
   Почему релевантно: upstream contract и deployment model MCPace.

6. ABP upstream repo  
   https://github.com/theredsix/agent-browser-protocol  
   Почему релевантно: upstream REST/MCP endpoints и cross-platform build context для ABP.

7. Docker Compose docs  
   https://docs.docker.com/compose/  
   Почему релевантно: официальный reference для Plan B, если проект вырастет из single-container runtime.

8. Node.js child_process docs  
   https://nodejs.org/api/child_process.html  
   Почему релевантно: база для оценки варианта полного rewrite на Node.js CLI.

## 2) Сравнение

### Ключевые совпадения

- Все варианты могут управлять Docker и внешними процессами.
- Все варианты могут генерировать клиентские конфиги и health checks.
- Все варианты могут поддерживать Windows/Linux/macOS при аккуратной реализации.

### Ключевые различия и компромиссы

- PowerShell 7 evolution лучше сохраняет уже существующую кодовую базу и даёт самый дешёвый migration path.
- Node.js rewrite даёт более привычный cross-platform CLI runtime, но требует почти полного переписывания launcher logic и новой surface area для багов.
- Compose-first хорошо упрощает контейнерную часть, но не решает orchestration host-процесса ABP и не убирает потребность в launcher logic целиком.

## 3) Матрица

| Вариант | Внедрение | Поддержка | Риски | Производительность | Совместимость | Лицензия / стоимость |
|---|---|---|---|---|---|---|
| PowerShell 7 evolution поверх текущего кода | Низкая цена, можно внедрять инкрементально | Высокая, если держать `lib/runtime.ps1` единой точкой abstractions | Средние: PowerShell-специфичная отладка и нужен runtime test matrix | Достаточно для launcher-задач | Хорошая для Windows/Linux/macOS при запуске через `pwsh` | OSS, без доп. стоимости |
| Full rewrite на Node.js CLI | Высокая цена, почти новый проект | Потенциально высокая после стабилизации | Высокие: rewrite-регрессии, новая CLI-поверхность, миграция скриптов и docs | Хорошая | Отличная cross-platform | OSS, без доп. стоимости |
| Compose-first + тонкий launcher | Средняя цена | Средняя, если контейнеров станет больше одного | Средние: остаётся отдельное управление host ABP | Хорошая для container part | Хорошая для контейнеров, но не закрывает host process orchestration | OSS, без доп. стоимости |

## Выбор и аргументация

**Выбор: PowerShell 7 evolution поверх текущего launcher-а.**

Почему:

- это самый обратимый путь;
- он сразу убирает главные проблемы текущего репозитория: Windows-only assumptions, секреты в git, drift клиентских конфигов;
- он не требует выбросить уже работающую логику запуска/проверки/backup/autostart.

## Plan B

**Plan B: Compose-first для container layer + сохранение thin PowerShell launcher для host ABP.**

Когда это станет выгодно:

- появится второй обязательный контейнер;
- появится необходимость формально описать volumes/networks/env dependencies;
- появится CI/CD для локального dev stack.

## Что критично проверить перед окончательным решением

1. Реальный прогон `pwsh ./start.ps1` на Windows и Linux.
2. Работоспособность `--add-host host.docker.internal:host-gateway` в целевых Linux install base.
3. Поведение ABP package на macOS и Linux в реальном окружении пользователя.
4. Нужен ли пользователю direct client registration отдельных серверов, или одного `mcpace` endpoint достаточно.

## При каких условиях выбор стоит пересмотреть

- launcher начнёт управлять несколькими контейнерами и сложной сетью;
- появится необходимость сервисного режима с daemon/agent lifecycle вне интерактивного shell;
- PowerShell runtime станет ограничением для CI, packaging или team onboarding.
