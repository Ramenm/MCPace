# MCPace: базовый Linux-чек-лист проверки проекта

Этот чек-лист нужен, чтобы не проверять проект “на глаз”. Идеальный результат: один отчёт показывает, что установка, автозапуск, MCP endpoint, upstream MCP-серверы, безопасность, npm/release и дистрибутивная совместимость реально проходят.

## 0. Одной командой

```bash
npm run verify:linux:auto:host
```

На машине с Docker:

```bash
npm run verify:linux:auto:full
```

Для release:

```bash
npm run verify:linux:auto:release
```

Статус `pass` — можно двигаться дальше. `warn` — для разработки допустимо, но перед релизом каждое предупреждение нужно закрыть или явно объяснить. `fail` — релиз блокируется.

## 1. Чистый старт обычного пользователя

```bash
rm -rf /tmp/mcpace-user-test
mkdir -p /tmp/mcpace-user-test
npm install -g @mcpace/cli
mcpace version
mcpace doctor --json --root /tmp/mcpace-user-test
```

Проверить:

- команда находится в `PATH`;
- версия совпадает с release/package version;
- `doctor` не требует ручного JSON-редактирования;
- ошибки показывают executable, cwd, args, exit code, stderr tail и timeout.

## 2. Linux bootstrap почти автоматически

Из repo/root:

```bash
scripts/linux-auto-setup.sh \
  --root /tmp/mcpace-user-test \
  --bin ./target/release/mcpace \
  --skip-client-install
```

Проверить:

- создаются config/state директории;
- права на config/state не слишком широкие;
- endpoint остаётся на `127.0.0.1`;
- `/healthz` отвечает;
- `mcpace service status --json` показывает реальное состояние.

## 3. systemd user autostart

```bash
systemctl --user daemon-reload
systemctl --user is-enabled --quiet mcpace.service
systemctl --user restart mcpace.service
curl -fsS http://127.0.0.1:39022/healthz
```

Проверить:

- есть `~/.config/systemd/user/mcpace.service`;
- сервис не просто записан файлом, а реально enabled через `systemctl --user enable`;
- unit содержит `Restart=on-failure`, `NoNewPrivileges=true`, `UMask=077`, `WantedBy=default.target`;
- `ExecStart` корректно quoting/escaping для пробелов, `$` и `%`;
- если нужен старт до логина или после logout — отдельно документируется `loginctl enable-linger $USER`;
- WSL/containers без user systemd дают warning, а не ложный pass.

## 4. MCP HTTP smoke

```bash
mcpace serve stop --json --root /tmp/mcpace-user-test || true
mcpace setup --json --root /tmp/mcpace-user-test --host 127.0.0.1 --port 39022 --skip-client-install
```

Проверить raw/client flow:

- `POST /mcp initialize` возвращает валидный MCP response;
- если вернулся `MCP-Session-Id`, он используется в следующих запросах;
- `notifications/initialized` принят;
- `tools/list` содержит ожидаемые tools;
- неподдержанный `GET /mcp` отвечает предсказуемо;
- invalid Host / duplicate Host / conflicting Content-Length / плохой Content-Type не проходят.

## 5. Upstream MCP servers

```bash
mcpace server list --json --root /tmp/mcpace-user-test
mcpace server test filesystem --refresh --timeout-ms 30000 --json --root /tmp/mcpace-user-test
mcpace server test context7 --refresh --timeout-ms 30000 --json --root /tmp/mcpace-user-test
mcpace server test exa --refresh --timeout-ms 30000 --json --root /tmp/mcpace-user-test
mcpace server test serena --refresh --timeout-ms 120000 --json --root /tmp/mcpace-user-test
```

Проверить:

- `npx` upstream имеют `env_vars` для npm registry/proxy/certs/cache;
- API keys пробрасываются по имени переменной, а не значением в JSON;
- Serena тестируется на реальном projectRoot, не на случайном temp;
- disabled servers не считаются сломанными;
- stderr tail и exit code видны при падении.

## 6. npm/release/package proof

```bash
npm run lint:npm
npm run test:npm
npm run test:repo:smoke
npm run verify:release-targets
npm run verify:platform-packages
npm run verify:npm-pack
npm run verify:secrets -- --json
```

Проверить:

- Unix binary в npm pack имеет executable bit;
- Linux packages не заявляют Alpine/musl, пока musl target не включён;
- release archive не включает `.claude`, `.codex`, `.omc`, `%SystemDrive%`, screenshots, `node_modules`, `target`;
- secret scan — `0 findings`.

## 7. Дистрибутивы Linux

Минимально проверить:

- Ubuntu текущий LTS x64;
- Ubuntu/Debian со старейшей glibc, которую хотите заявлять;
- Linux ARM64;
- Alpine только если есть `linux-*-musl` package и отдельный proof.

Проверки:

```bash
npm run verify:linux:auto:full
npm run test:linux-npm-install:docker
```

Перед релизом записать максимальный `GLIBC_*` symbol version собранного binary.

## 8. Безопасность

Проверить:

- bind по умолчанию только `127.0.0.1`;
- `0.0.0.0` запрещён или требует явной auth-модели;
- session id не считается auth;
- Origin/Host validation включена;
- upstream child env allowlisted;
- секреты не пишутся в JSON/logs/stderr reports;
- config/state/cache лежат в XDG-compatible местах;
- release zip чистый от локального machine-state.

## 9. Итоговое правило

Проект можно считать готовым для Linux только если:

```text
verify:linux:auto:release    pass
verify:npm-pack              pass
verify:secrets               pass
MCP runtime smoke            pass
enabled upstream servers     pass или documented disabled
systemd user autostart       pass на обычной Linux-сессии
Docker distro proof          pass для заявленных дистрибутивов
```
