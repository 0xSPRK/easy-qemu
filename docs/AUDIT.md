# Аудит easy-qemu v0.2 → v0.3 (Фаза 0)

**Дата:** 2026-10-21
**Объём:** весь репозиторий (Rust core, Tauri-обвязка, Svelte UI, CI, конфигурация репозитория)
**Характер:** аудит без изменения кода. Единственный добавленный файл — этот документ.
**Лицензия проекта:** GPL-3.0-only

---

## 0. Методика и состояние дерева

Аудит проведён по **рабочему дереву** (не по `HEAD`). На момент аудита дерево содержит крупный
незакоммиченный набор изменений (22 изменённых файла, +3996/−876 строк) — это, судя по истории,
незавершённая работа над v0.2 (мультидиск, SSH, статистика, live-снапшоты):

```
M Cargo.lock, README.md
M crates/core/{Cargo.toml,src/{config,lib,manager,qemu,qmp,snapshots,store,vm}.rs}
M src-tauri/src/{commands,lib}.rs
M ui/src/components/{DetailsPanel,SettingsModal,SnapshotsModal,VmForm,VmTable}.svelte
M ui/src/lib/{api,types}.ts
M ui/src/views/{ConsoleView,MainView}.svelte
?? crates/core/src/qemu/tests.rs, crates/core/src/stats.rs, ui/src/lib/novnc.d.ts
```

> **Важно для планирования:** сначала нужно решить, что делать с этим незакоммиченным набором
> (закоммитить как `feat: v0.2 ...` до начала Фазы 1, или откатить). Иначе любой дифф/PR
> v0.3 будет смешан с v0.2.

**Проверка сборки:** `cargo test -p easy-qemu-core` → **74 passed, 0 failed** (0.18 s).
Полный `cargo clippy --workspace --all-targets -- -D warnings` и `svelte-check` в рамках аудита
не запускались (долгая компиляция таури-зависимостей); замеры ниже — по коду.

**Что подтверждено чтением исходников, а не догадками:** каждая находка ниже содержит
`файл:строка`. Гипотезы, которые **не подтвердились**, помечены явно.

---

## 1. Карта модулей

```
easy-qemu/
├── crates/core/                 ← UI-free ядро (workspace member, 12 файлов)
│   ├── vm.rs         (1282)    ← домен: Vm, VmDraft, DiskEntry, перечисления, валидация
│   ├── manager.rs    (1077)    ← оркестратор: CRUD VM, старт/стоп, статусы, снапшоты, статистика
│   ├── qemu.rs        (764)    ← сборка CLI QEMU, поиск бинарей, запуск процесса
│   │   └── qemu/tests.rs (467) ← тесты сборки аргументов
│   ├── qmp.rs         (439)    ← асинхронный QMP-клиент (JSON line protocol поверх TCP)
│   ├── proxy.rs       (295)    ← websockify-прокси (axum) noVNC → VNC
│   ├── store.rs       (365)    ← хранилище: vms/*.json, running.json, удаление VM
│   ├── config.rs      (136)    ← config.toml (TOML)
│   ├── snapshots.rs   (180)    ← офлайн-снапшоты через qemu-img
│   ├── stats.rs       (131)    ← CPU% хоста + block/balloon из QMP
│   ├── process.rs      (97)    ← живость PID, принудительное завершение
│   ├── accel.rs       (123)    ← определение KVM/WHPX/TCG
│   ├── env.rs          (64)    ← проба окружения (бинарники, акселераторы) для UI
│   ├── ssh.rs         (130)    ← запуск ssh в внешнем терминале
│   └── lib.rs          (18)
├── src-tauri/                   ← Tauri-обвязка (workspace member)
│   ├── src/lib.rs      (88)     ← Builder, capability, poll_loop, emit "vm:statuses"
│   ├── src/commands.rs (331)    ← 26 #[tauri::command]
│   ├── tauri.conf.json          ← CSP=null, bundle targets=["nsis"] (только Windows!)
│   └── capabilities/default.json← core:default + dialog:default
├── ui/                          ← Svelte 5 + TS + Vite
│   ├── src/views/MainView.svelte, ConsoleView.svelte
│   ├── src/components/{VmForm,VmTable,DetailsPanel,SettingsModal,SnapshotsModal,LogsModal,StatusBar,Toasts,Modal}.svelte
│   └── src/lib/{api,types,state,toast,hotkeys}.ts
├── .github/workflows/ci.yml     ← единственный workflow (только windows-latest)
├── .claude/CLAUDE.md            ← шаблон + автогенерируемый блок repowise
├── .mcp.json, .vscode/mcp.json  ← MCP-конфиг с абсолютным локальным путём
└── .vscode/extensions.json
```

### Потоки данных (сквозной маршрут)

**1. Создание VM**
```
UI VmForm.svelte → api.createVm(draft) → invoke("create_vm")
  → commands.rs:31 create_vm → Manager::create (manager.rs:302)
    → VmDraft::validate (vm.rs:972)                     ← только логика, без FS
    → для каждого DiskSpec::New:
        resolve_binary("qemu-img") (qemu.rs:31)
        qemu::create_disk (qemu.rs:736) → qemu-img create -f qcow2 <path> <size>G
        → DiskEntry{owned:true, path:<storage>/<name>/<id>-d<i>.qcow2}
    → для каждого DiskSpec::Existing: проверка расширения + is_file() → DiskEntry{owned:false}
    → Vm → Store::save_vm (store.rs:128) → vms/<id>.json (tmp + rename)
  → emit_statuses → app.emit("vm:statuses")
```

**2. Сборка командной строки QEMU**
```
Manager::start (manager.rs:496)
  → resolve_binary("qemu-system-x86_64") + accel_support_for (accel::probe → `-accel help`)
  → qemu::start_vm (qemu.rs:622)
    → free_tcp_port() (qemu.rs:74)                        ← TOCTOU
    → QemuLaunch { qmp_port, pidfile, vars_fd, vnc_bind, hostfwd_bind, accel_resolved }
    → build_qemu_args (qemu.rs:119) → Vec<String>        ← ~30 секций: name/machine/smp/cpu/
                                                      disks/-nic/video/audio/rtc/boot/uefi/
                                                      cdrom/objects/qmp/pidfile/incoming
    → Command::new(bin).args(&args)                      ← массив, без shell
      Windows: DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP (qemu.rs:652-656)
      stdout+stderr → <base>/<id>.log (truncate)          (qemu.rs:579-650)
```

**3. Запуск и готовность**
```
wait_for_pidfile (qemu.rs:699)  — до 40×250 мс, читает <id>.pid
  → read_pidfile (qemu.rs:726)
wait_qmp_vnc (qemu.rs:685)      — до 10×500 мс: Qmp::connect → qmp_capabilities → query-vnc
  → RunningInfo{id,pid,qmp_port,vnc_port,display,started_at}
  → Manager.running + Store::save_running → running.json
```

**4. QMP**
```
Qmp::connect (qmp.rs:21)  — TCP 127.0.0.1:<qmp_port>, connect-timeout 2 s, IO-timeout 3 s
  → read greeting → exec("qmp_capabilities")              ← обязательная первая команда
Qmp::exec (qmp.rs:34)     — {"execute":cmd,"arguments":..} → цикл чтения:
                            события отбрасываются, "error" → Err, "return" → Ok
Команды: stop/cont/system_reset/system_powerdown/suspend/system_wakeup/nmi/quit/
         send-key/screendump/query-vnc/query-status/query-block/query-blockstats/
         query-balloon/balloon/blockdev-*-medium/set_password/snapshot-*/query-jobs/migrate
```

**5. Статус-события в UI**
```
src-tauri/src/lib.rs:24 poll_loop — каждые 1500 мс → emit_statuses
  → Manager::refresh (manager.rs:207)
      для каждой running-VM: probe_status (manager.rs:922) — НОВОЕ TCP-соединение на каждый опрос
        ├─ QMP ответил и не "shutdown" → statuses[id] = Status::from_qmp
        └─ иначе → process::is_alive(pid,"qemu-system") → если мёртв → mark_dead
                   (снять запись, удалить pidfile, остановить прокси) → save_running
      каждый 3-й тик (≈4.5 с) → collect_stats (manager.rs:699):
        CPU% из GetProcessTimes//proc + QMP query-blockstats + query-balloon
      → snapshot_updates (manager.rs:266) → VmUpdate[]
  → app.emit("vm:statuses", updates) → UI MainView → таблица/детали/статус-бар
```

**6. Консоль (noVNC)**
```
UI ConsoleView.svelte → api.openConsole → invoke("open_console")
  → commands.rs:174 → Manager::open_console (manager.rs:645)
      → ProxyPool::ensure (proxy.rs:59): axum на 127.0.0.1:0 (эфемерный порт),
        маршрут GET /websockify → WebSocketUpgrade
      → создание окна "console_<id>" с index.html
UI: new RFB(container, "ws://127.0.0.1:<ws_port>/websockify", {shared:true})
  → axum ws_handler (proxy.rs:160) → proxy() → TcpStream::connect(127.0.0.1:<vnc_port>)
  → pump_ws_to_tcp / pump_tcp_to_ws (proxy.rs:179/195) — двунаправленный проксирование
Закрытие: Destroyed окна (lib.rs:35) → Manager::close_console → ProxyPool::stop
```

**7. Остановка**
```
UI → force_stop → Manager::force_stop (manager.rs:540)
  → process::kill_force (process.rs:41): сверка имени образа (tasklist / /proc/<pid>/comm),
    затем taskkill /PID <pid> /T /F (Windows) | kill -9 (Unix)
  → finish_stopped (manager.rs:551): стоп прокси, save_running, статус=Shutoff, удаление pidfile
```

**Хранение состояния**
```
<config>/config.toml           — настройки (qemu_binary, qemu_img, storage_dir, vnc_bind,
                                hostfwd_bind, theme)
<config>/vms/<id>.json         — запись VM (multi-disk, legacy disk_path мигрируется в normalize())
<config>/running.json          — карта running-VM (pid, qmp_port, vnc_port, started_at)
<config>/<id>.pid|.log|.sav    — pidfile, лог QEMU, сохранённое состояние
<config>/<id>-vars.fd          — OVMF VARS (per-VM)
<config>/disks/<name>/…        — диски (по умолчанию) или произвольная папка
<config>/screenshots/          — PNG-дампы
```

---

## 2. Содержимое служебных каталогов репозитория

### `.github/workflows/ci.yml` (46 строк, один workflow)
| Что | Факт | Оценка |
|---|---|---|
| ОС | только `windows-latest` (2 job: `core-tests`, `app-build`) | Linux не проверяется, хотя заявлена поддержка |
| fmt | `cargo fmt -p easy-qemu-core -- --check` | только core, не весь workspace |
| clippy | core — строго; **app — `|| true`** (`ci.yml:40`) | clippy приложения не может упасть — дыра в CI |
| тесты | `cargo test -p easy-qemu-core` | без `--workspace`; тесты Tauri-обвязки нет |
| UI | `npm ci` + `npm run build` в `ui/` | **нет `svelte-check`**, нет type-check |
| кэш | только npm (`actions/setup-node` c `cache: npm`) | нет кэша Rust (`Swatinem/rust-cache`) |
| аудит | нет | нет `cargo-audit` / `cargo-deny` / `npm audit` |
| релиз | нет | нет workflow по тегу `v*` |
| dependabot | нет | нет `.github/dependabot.yml` |
| сборка | `cargo build -p easy-qemu --release` | это **не** `tauri build`: нет NSIS-инсталлера и бандлинга; артефакт — голый `.exe` |
| root npm | `package-lock.json` в корне не устанавливается | локальный `@tauri-apps/cli` в CI недоступен |

**Вывод:** CI не является защитой от регрессий: clippy приложения не влияет на результат, Linux
не проверяется, type-check отсутствует, релиз не автоматизирован.

### `.claude/CLAUDE.md` (70 строк)
Шаблон-обёртка над автогенерируемым блоком **Repowise** (`<!-- REPOWISE:START -->` … `END`):
архитектура, «files that need care», команды. Проблемы:
* в разделе «Commands» указаны `npm run build` / `npm run dev` как команды сборки проекта —
  для Rust-части это неверно (нужны `cargo build/test/clippy`);
* блок содержит дату индексации и привязку к конкретному коммиту — устаревает;
* это персональный инструментарий конкретного разработчика в общем репозитории;
* полезная часть (команды, архитектура) отсутствует.

**Рекомендация:** заменить на вручную написанный `AGENTS.md` (команды сборки/теста, карта
модулей, правила), `CLAUDE.md` оставить симлинком/копией с ссылкой на `AGENTS.md`.
Автогенерируемый блок либо удалить, либо перенести в `docs/` и не коммитить.

### `.mcp.json` и `.vscode/mcp.json`
Оба содержат **машино-зависимый абсолютный путь** `D:/Projects/easy-qemu`:
```json
{"mcpServers":{"repowise":{"command":"repowise","args":["mcp","D:/Projects/easy-qemu","--transport","stdio"]}}}
```
На другой машине путь не существует → конфиг бесполезен; в публичном репозитории он бесполезен
тем более. **Убрать из git** (или параметризовать через `${workspaceFolder}` — но и тогда он
бесполезен без установленного `repowise`).

### `.vscode/extensions.json`
Рекомендует расширение `repowise-dev.repowise` — персональное, не обязательное. Можно оставить
(безвредно) или удалить. **Рекомендация:** оставить пустой/минимальный.

### `.gitignore` (8 строк)
`/target`, `/node_modules`, `/ui/dist`, `/ui/node_modules`, `*.log`, `.DS_Store`, `.repowise/`.
Проблемы: нет `.env`/`.env.*`, `*.local`, `.idea/`, `/target` (а не `target/`, что сломает
вложенные target-каталоги), нет `src-tauri/target`. Правило `*.log` выжигает и потенциально
нужные тестовые фикстуры. `.repowise/` есть — хорошо (артефакты индекса не в git: коммит
`659c9e0`).

---

## 3. Проверка гипотез A–J

| # | Гипотеза (из ТЗ) | Вердикт | Доказательство |
|---|---|---|---|
| **A** | CLI QEMU собирается безопасно: массив аргументов, без shell; пользовательские поля валидируются; нет инъекции опций (запятая в `-drive`/`-netdev`/`-chardev`, ведущий `-`) | **ЧАСТИЧНО ПОДТВЕРЖДЕНО** | ✅ Массив, без shell: `qemu.rs:647-650` `Command::new(bin).args(&args)`; shell нигде не используется. ✅ Типизированные поля: порты `u16`, proto — enum. ❌ Свободные строки интерполируются в comma-опции без экранирования: путь диска `qemu.rs:299-302` (`file={path},format=…` — запятая в пути ломает/инъецирует подопции), `tap_ifname`/`tap_br` `qemu.rs:372-378`, `hostfwd_bind` из конфига `vm.rs:527-535`, имя VM `qemu.rs:131`. ✅ Единственная выборочная проверка — `mem_path` на запятую (`qemu.rs:135`). ❌ Имя VM проверяется только на «не пусто» (`vm.rs:760-763`). ❌ **Функции «extra args» в проекте нет вообще** — гипотеза в части произвольных extra-args **не подтверждена** (искал по всему дереву: только `machine_extra` во внутренней сборке). |
| **B** | Удаление дисков: канонизация, только внутри каталога дисков, только manager-owned; нет traversal/symlink | **НЕ ПОДТВЕРЖДЕНО (находки F-01…F-03)** | ❌ `store.rs:154-166`: удаляется `d.path` при `d.owned==true`; **ни канонизации, ни проверки «внутри каталога дисков», ни проверки symlink нет**. ❌ Флаг `owned` приходит из JSON-файла и из payload'а фронтенда: `update()` без фильтрации копирует `patch.disks` (`manager.rs:410`) → `owned:true` можно поставить на любой путь. ❌ `remove_file` по пути, где родительский каталог — symlink, удалит файл за пределами. ✅ Само удаление `owned:false` не трогает (`manager.rs:346-350`, тест `keep_disks_flag_preserves_files`). |
| **C** | noVNC-прокси: только 127.0.0.1, эфемерный порт, одноразовый токен/проверка Origin, недоступен другим локальным процессам/страницам; риск VNC без пароля | **ЧАСТИЧНО ПОДТВЕРЖДЕНО (F-04, F-05)** | ✅ `proxy.rs:66` — bind строго `("127.0.0.1", 0)`, порт эфемерный. ✅ Прокси останавливается при остановке VM (`manager.rs:262`, `manager.rs:552`) и при закрытии окна (`lib.rs:35-48`). ❌ **Ни токена, ни проверки Origin нет**: `ws_handler` (`proxy.rs:160-162`) принимает любое соединение → любой локальный процесс и **любая веб-страница в браузере** может подключиться к `ws://127.0.0.1:<port>/websockify` и получить полный контроль (клавиатура/мышь) над гостевой ОС. ❌ VNC по умолчанию **без пароля** (`vnc_password_auth` default false, `vm.rs:684`), QEMU слушает raw-VNC на 127.0.0.1 → любой локальный процесс подключается напрямую, минуя прокси. ❌ `vnc_bind` настраивается на `0.0.0.0` без предупреждения (`config.rs:71-73`). ❌ Пароль VNC непригоден: noVNC в UI не передаёт учётные данные (`ui/src/views/ConsoleView.svelte:33-37`) → фича «VNC password authentication» в README фактически не работает, т.е. практический режим — без пароля. |
| **D** | Tauri: строгий CSP, минимальные capabilities, нет лишних fs/shell/http, команды валидируют вход | **ЧАСТИЧНО ПОДТВЕРЖДЕНО (F-09, F-19, F-02)** | ✅ Capabilities минимальны: `core:default` + `dialog:default` (`src-tauri/capabilities/default.json`) — **fs/shell/http не выданы** (проверено по `src-tauri/gen/schemas/capabilities.json`). ✅ `withGlobalTauri: false`. ❌ **CSP = null** (`src-tauri/tauri.conf.json:25-27`) → webview грузит что угодно, инъекция скрипта = вызов всех 26 команд. ❌ Идентификаторы команд не валидируются: `id: String` → сквозной path traversal (F-02). ❌ `pick_path` — **синхронная** команда с `blocking_pick_file` (`commands.rs:313-330`) → блокирует UI-поток. ❌ `read_log` — синхронная команда, читает файл целиком (`commands.rs:213-216`). |
| **E** | config/VM-records/running.json пишутся атомарно (tmp + fsync + rename), нет порчи; параллельные записи защищены | **ЧАСТИЧНО ПОДТВЕРЖДЕНО (F-13, F-12, F-31)** | ✅ tmp+rename есть во всех трёх местах: `store.rs:128-134` (vms/*.json), `store.rs:233-238` (running.json), `config.rs:63-69` (config.toml). ❌ **Нет fsync** ни файла, ни каталога → при потере питания возможна пустая/битая запись (rename атомарен только метаданно). ❌ `load_running` использует `unwrap_or_default()` (`store.rs:229`) → **битый running.json молча превращается в пустую карту** → приложение «забывает» живые QEMU → осиротевшие процессы. ❌ Имя tmp-файла фиксировано (`.running.json.tmp`) → при параллельной записи файлы конфликтуют. ❌ Нет блокировки на уровне файла/процесса → два экземпляра приложения пишут одну папку. |
| **F** | running.json: обработка устаревших записей (PID, переиспользование PID, «сокет QMP не отвечает»); нет зомби при закрытии приложения | **ЧАСТИЧНО ПОДТВЕРЖДЕНО (F-11, F-15, F-12)** | ✅ Устаревшие записи подчищаются: `refresh()` → `probe_status` → если QMP не отвечает, проверяется `process::is_alive` → `mark_dead` (`manager.rs:207-263`). ✅ «QMP не отвечает, но процесс жив» — статус сохраняется, VM не считается мёртвой. ✅ Переподключение после перезапуска приложения возможно: QEMU продолжает работать, QMP-порт из running.json. ❌ **Защита от переиспользования PID слабая**: сверяется только префикс имени образа `qemu-system` (`process.rs:33-37`) → процесс *чужого* QEMU с тем же PID будет признан «нашим» и может быть **убит** в `force_stop` (`manager.rs:540-548`). Поле `started_at` сохраняется, но для валидации PID **не используется**. ❌ Старый pidfile не удаляется перед стартом → `wait_for_pidfile` может прочитать **прошлый** PID и вернуть успех (`qemu.rs:699-724`); сам pidfile читается без ретрая → на частично записанном файле ошибка парсинга (`qemu.rs:726-733`). ❌ Зомби: QEMU **не** завершается при выходе из приложения (by design — есть reattach), но при битом running.json reattach невозможен → осиротевший QEMU без UI-управления. |
| **G** | QMP: таймауты, переподключение, обработка событий/ошибок, нет блокировки UI-потока; named pipe (Windows) / unix socket (Linux) корректно закрываются | **ЧАСТИЧНО ПОДТВЕРЖДЕНО; часть гипотезы не применима (F-10, F-16)** | ✅ Таймауты есть: connect 2 с, IO 3 с (`qmp.rs:11-12`). ✅ UI-поток не блокируется: все команды `async`, Tauri выполняет их на async-рантайме. ✅ Ошибки QMP превращаются в `Err` (`qmp.rs:47-49`). ✅ Переподключение реализовано «с нуля» на каждую операцию (новая сессия на команду). ❌ **Named pipe / unix socket НЕ используются**: QMP поднимается как TCP на 127.0.0.1 (`qemu.rs:17-29`) → любой локальный процесс может подключиться к монитору и взять VM под контроль (`quit`, `human-monitor-command`); аутентификации нет, порт лежит в `running.json`. ❌ **События QMP не используются**: `exec` отбрасывает всё с ключом `"event"` (`qmp.rs:42-46`) → статус берётся исключительно опросом (1.5 с) вместо реактивных событий SHUTDOWN/RESET. ❌ **Конкуренция соединений**: `-chardev socket,server=on` принимает одного клиента; опрос создаёт новое соединение каждые 1.5 с на каждый VM (`manager.rs:922`), `collect_stats` — ещё одно (`manager.rs:737`) → соединения встают в очередь и могут отвалиться по таймауту. |
| **H** | Единый тип ошибок (thiserror), понятные сообщения в UI, нет unwrap/expect/panic на пользовательских путях | **НЕ ПОДТВЕРЖДЕНО как сформулировано (F-23, F-24)** | ❌ Типизированных ошибок нет: везде `anyhow::Result` + `bail!` с текстом; `thiserror` в зависимостях нет. Ошибки **читаемые** (английские, человекочитаемые) и пробрасываются в UI как строка (`commands.rs:14-16` `format!("{e:#}")`) — это плюс. ❌ `unwrap/expect` на пользовательских путях есть: `proxy.rs:122` `.expect("failed to create tokio runtime for websockify")`, `proxy.rs:125` `.unwrap()`, `qemu.rs:594-596` и `qemu.rs:603` — **паника** в fallback'е открытия лога (`File::create(...).unwrap()`), `src-tauri/src/lib.rs:87` `.expect("error while running Easy QEMU")` (штатный boilerplate Tauri). |
| **I** | Структурированное логирование (tracing/log), ротация/ограничение размера логов QEMU | **НЕ ПОДТВЕРЖДЕНО (F-14, F-20)** | ❌ Ни `tracing`, ни `log` в зависимостях нет (`crates/core/Cargo.toml`). Диагностика приложения — `eprintln!` (`store.rs:105`, `src-tauri/src/lib.rs:19`) → в GUI-приложении на Windows теряется. ❌ Лог QEMU: `<base>/<id>.log`, при каждом старте **обрезается** (`truncate(true)`, `qemu.rs:579-586`) и растёт без ограничений и без ротации. ❌ `log_tail` читает файл **целиком** в память (`qemu.rs:608-615`), а `read_log` — синхронная команда → на большом логе UI подвисает. |
| **J** | Поиск бинарников QEMU: PATH, типовые каталоги (Windows `Program Files\qemu`; Linux `/usr/bin`), понятная диагностика при отсутствии QEMU/OVMF/swtpm | **ЧАСТИЧНО ПОДТВЕРЖДЕНО (F-21)** | ✅ `resolve_binary` (`qemu.rs:31-71`): override → PATH → `C:\Program Files\qemu` (только Windows). ✅ На Linux `/usr/bin` покрывается через PATH; типовых каталогов Fedora (`/usr/libexec`) нет. ✅ OVMF ищется по списку каталогов + рядом с бинарём (`qemu.rs:512-545`), сообщение об отсутствии понятное («UEFI requested but OVMF was not found…»). ✅ Есть проба окружения для UI: `env.rs` + команда `probe_env` (`commands.rs:306-310`) → панель «Detected environment». ❌ Путь-override проверяется через `exists()` — **каталог тоже пройдёт** (`qemu.rs:33`). ❌ В сообщении об ошибке не перечислены проверяемые места. ❌ **swtpm не поддерживается вообще** — ни поиска, ни опции; упоминать его в диагностике нечего (гипотеза в части swtpm **не применима**). |

---

## 4. Таблица находок

Severity: **critical** — немедленный эксплуатируемый ущерб/потеря данных; **high** — эксплуатируется
при обычных или правдоподобных условиях; **medium** — усиливает риск/ломает надёжность; **low** — гигиена.
Effort: S ≈ <0.5 дня, M ≈ 0.5–2 дня, L ≈ >2 дней.

| ID | Sev | Файл:строка | Описание | Предложение | Effort |
|---|---|---|---|---|---|
| F-01 | **critical** | `crates/core/src/manager.rs:312-315` | **Path traversal через имя VM.** Имя VM используется как компонент пути: `cfg.storage_dir(&base).join(&v.name)`. Имя не валидируется (`vm.rs:760-763` — только «не пусто»). `PathBuf::join` с абсолютным путём **заменяет** базу, а `..` даёт выход вверх: имя `C:\Windows\Temp` → диск создаётся вне каталога дисков; имя `..\..\victim` → выход из storage. Такой диск помечается `owned:true` → потом удаляется при `delete_vm`. | Валидировать имя: белый список символов `[A-Za-z0-9._-]`, длина ≤ 64, запрет `.`/`..`/разделителей/двоеточия. Канонизировать storage dir и проверять, что итоговый путь диска — внутри него. Само имя в каталог не подставлять (использовать `id`). | M |
| F-02 | **high** | `crates/core/src/store.rs:118-125`, `206-220`, `69-81`; `src-tauri/src/commands.rs:54-62,106,145` | **Path traversal через параметр `id`.** `id` из UI не проверяется и подставляется в `vms_dir.join(format!("{id}.json"))`, `base.join(format!("{id}.pid"))` и т.д. `id = "../../secret"` → `remove_file` удалит произвольный `.json`/`.pid`/`.fd`/`.log`/`.sav`; `get_vm` прочитает произвольный JSON. | Валидировать формат `id` (regex `^vm[0-9a-f]{32}$`) в одной точке (Tauri-слой + core); дополнительно проверять, что итоговый путь внутри `base`. | S |
| F-03 | **high** | `crates/core/src/store.rs:152-167`; `crates/core/src/manager.rs:410` | **Удаление произвольных файлов «manager-owned».** Условие — только флаг `owned` в записи. Флаг приходит из пользовательского JSON **и** из payload'а фронтенда (`update()` копирует `patch.disks` без фильтрации), поэтому `owned:true` можно присвоить любому пути; канонизации/проверки «внутри каталога дисков»/проверки symlink нет. | Удалять диск только если: (а) путь канонизирован и лежит внутри storage dir, (б) файл — обычный файл (не symlink/reparse), (в) `owned` подтверждён самим менеджером (например, отдельным манифестом manager-owned дисков), а не полем из JSON. | M |
| F-04 | **high** | `crates/core/src/proxy.rs:59-112`, `160-162`; `crates/core/src/manager.rs:645-665` | **Прокси консоли без аутентификации.** Любой локальный процесс и любая веб-страница браузера может подключиться к `ws://127.0.0.1:<port>/websockify` и получить полный контроль над гостевой ОС (клавиатура/мышь). WS-соединения на localhost не блокируются браузерами. | Одноразовый токен на сессию в URL/заголовке (`?token=…`), проверка `Origin` (только `tauri://localhost`/`http://localhost:1420`), опционально — Basic-auth. Плюс строгая CSP (F-09). | M |
| F-05 | **high** | `crates/core/src/vm.rs:684`; `crates/core/src/qemu.rs:393-405`; `crates/core/src/config.rs:71-73`; `ui/src/views/ConsoleView.svelte:33-37` | **VNC без пароля по умолчанию** + настраиваемый `vnc_bind` (в т.ч. `0.0.0.0`) без предупреждения; фича пароля фактически нерабочая (noVNC не передаёт учётные данные), т.е. пользователь вынужден оставить VNC открытым. | Пароль VNC — рабочий путь (передавать в noVNC) либо требовать явного подтверждения «VNC без пароля»; при `vnc_bind` ≠ 127.0.0.1 — явное предупреждение в UI. | M |
| F-06 | **high** | `crates/core/src/config.rs:74-82`; `crates/core/src/vm.rs:527-535`; `crates/core/src/qemu.rs:364-370` | **hostfwd по умолчанию слушает все интерфейсы** (`hostfwd_bind` пустой → `hostfwd=tcp::2222-:22` → 0.0.0.0). Гостевые службы публикуются в локальную сеть без предупреждения. | По умолчанию `127.0.0.1`. Значение `0.0.0.0`/пустое — только по явному выбору, с подтверждением в UI («открыть в локальной сети»). Учесть в миграции: у старых конфигов поле отсутствует → новое поведение применится автоматически (это изменение поведения, отметить в CHANGELOG). | S |
| F-07 | **high** | `crates/core/src/ssh.rs:56-62` (+ `vm.rs:538-554`) | **Инъекция команд в Windows-лаунчере SSH.** Fallback запускает `cmd /C ssh <args>`; cmd.exe выполняет остаток командной строки как одну команду → спецсимволы в `host`/`user` (`&`, `|`, `>`, `^`) превращают цель в цепочку команд. Значения берутся из пользовательской записи VM. | Не использовать `cmd /C`: запускать `ssh.exe` напрямую с `CREATE_NEW_CONSOLE` (аргументы уже передаются массивом). Дополнительно — валидация `host`/`user` (allow-list символов). | S |
| F-08 | **medium** | `crates/core/src/qemu.rs:299-302` (путь диска), `372-378` (tap ifname/br), `vm.rs:527-535` (hostfwd bind), `qemu.rs:131` (имя VM) | **Инъекция подопций QEMU через запятые.** Свободные строки подставляются в comma-опции без экранирования: `file=<путь>` (запятая в пути ломает `-drive` и позволяет добавить подопции), `ifname=`, `br=`, `hostfwd=`, `-name`. Экранирование в QEMU — удвоение запятой (`,,`). | Единый модуль валидации + типизированный builder аргументов; экранировать/запрещать запятые в значениях; проверить `-name` на запятую. | M |
| F-09 | **medium** | `src-tauri/tauri.conf.json:25-27` | **CSP = null.** Любой внедрённый скрипт в webview может вызвать все 26 команд (удалить VM, стартовать, читать логи, запустить ssh). | Строгая CSP: `default-src 'self'; connect-src 'self' ws://127.0.0.1:* ipc: http://ipc.localhost; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'` — согласовать с noVNC (worker/blob). | S |
| F-10 | **medium** | `crates/core/src/qemu.rs:17-29`; `crates/core/src/qmp.rs:21-32` | **QMP-монитор доступен по TCP без аутентификации** → любой локальный процесс может подключиться и управлять VM (`quit`, `human-monitor-command`). Порт публикуется в `running.json`. | Windows: named pipe; Linux: unix socket в приватном каталоге с правами 0600. Как минимум — случайный порт + проверка, что соединение локальное. | L |
| F-11 | **medium** | `crates/core/src/process.rs:33-37`; `crates/core/src/manager.rs:540-548` | **Слабая защита от переиспользования PID.** Сверяется только префикс имени образа (`qemu-system`): чужой QEMU с тем же PID будет признан «своим» и **убит** через `taskkill /F`. Поле `started_at` в `RunningInfo` не используется для валидации. | Хранить и сверять время старта процесса (Windows `GetProcessTimes` creation time; Linux `/proc/<pid>/stat` поле 22) — уже есть код в `stats.rs`, переиспользовать. | M |
| F-12 | **medium** | `crates/core/src/store.rs:224-231` | **Битый `running.json` = потеря контроля над живыми QEMU.** `serde_json::from_str(...).unwrap_or_default()` молча превращает мусор в пустую карту → приложение не знает о работающих VM → они становятся осиротевшими (убить можно только вручную). | При ошибке парсинга: сохранить файл как `.corrupt`, показать предупреждение в UI, **не** молча терять записи. | S |
| F-13 | **medium** | `crates/core/src/store.rs:128-134`, `233-238`; `crates/core/src/config.rs:63-69` | **Нет fsync при «атомарной» записи.** tmp+rename без `sync_all` файла и без fsync каталога → при потере питания возможна пустая/обрезанная запись (на ext4 rename атомарен, но данные не гарантированы). Имя tmp-файла фиксировано → конфликт параллельных писателей. | write → `file.sync_all()` → rename → fsync каталога; уникальное имя tmp (pid+counter). | S |
| F-14 | **medium** | `crates/core/src/qemu.rs:579-605`, `608-615`; `src-tauri/src/commands.rs:213-216` | **Лог QEMU без ограничения размера и ротации** + читается целиком в память синхронной командой → рост диска и подвисание UI на большом логе. | Ограничить размер (например 8 МБ, ring-buffer: при превышении — обрезка/ротация `x.log.1`); читать только хвост (seek с конца); `read_log` → `async`. | M |
| F-15 | **medium** | `crates/core/src/qemu.rs:699-724`, `726-733` | **Старый pidfile не удаляется перед стартом** → `wait_for_pidfile` считывает **прошлый** PID и возвращает «успех» (приложение считает VM запущенной с чужим PID). Плюс pidfile читается без ретрая → на частично записанном файле ошибка парсинга. | Удалять pidfile перед стартом; читать с ретраем; сверять PID с PID дочернего процесса (Windows не даёт — тогда сверять время старта процесса). | S |
| F-16 | **medium** | `crates/core/src/manager.rs:207-246`, `699-771`; `crates/core/src/qemu.rs:685-697` | **Конкуренция QMP-соединений.** `-chardev socket,server=on` обслуживает одного клиента; опрос (каждые 1.5 с) и сбор статистики создают новые соединения → соединения встают в очередь и отваливаются по таймауту (3 с). | Одно персистентное соединение на VM с reconnect; либо мьютекс на QMP + очередь. | M |
| F-17 | **medium** | `crates/core/src/vm.rs:1030-1051`; `crates/core/src/manager.rs:311-321` | **Размер диска (`size_gb`) нигде не валидируется.** В UI слайдер 4–512 ГБ, но backend принимает любое `u32` (включая 0 и 4 ТБ) → создаётся разреженный образ. | Диапазон 1..4096 ГБ, кратно 1; проверка в `VmDraft::validate`. | S |
| F-18 | **medium** | `crates/core/src/manager.rs:827-851` vs `crates/core/src/snapshots.rs:104-119` | **Имя live-снапшота не валидируется** (офлайн-путь валидирует). Имя попадает в job-id QMP (`eq-<name>-<uuid>`) с пробелами/точкой. | Переиспользовать `valid_snapshot_name` в live-вариантах. | S |
| F-19 | **medium** | `src-tauri/src/commands.rs:313-330` | **Блокирующий диалог в синхронной команде** (`blocking_pick_file`) → блокировка UI-потока на весь диалог (на Windows — «冻结» окна). | Сделать команду `async` и использовать неблокирующий API диалога. | S |
| F-20 | **medium** | `crates/core/Cargo.toml`; `src-tauri/src/lib.rs:19`; `crates/core/src/store.rs:105` | **Нет структурированного логирования.** Вся диагностика — `eprintln!`, в GUI-приложении на Windows теряется. | `tracing` + `tracing-subscriber` (fmt + file, с ротацией), уровень в настройках. | M |
| F-21 | **medium** | `crates/core/src/qemu.rs:31-54`, `56-71`, `512-545` | **Поиск бинарей и диагностика.** Override-проверяется через `exists()` (каталог пройдёт); в ошибке не указано, где искали; на Windows только `C:\Program Files\qemu`; нет `/usr/libexec/qemu-*`; swtpm не поддерживается. | `is_file()`; в ошибке перечислять проверенные пути; расширить список каталогов; в панели окружения показывать, что swtpm не поддерживается. | S |
| F-22 | **medium** | `crates/core/src/vm.rs:1042-1050` | **Проверка расширения диска — пустая ветка** (`if` без тела, только комментарий) → мёртвый код, создающий иллюзию валидации. | Реализовать проверку (qcow2/qcow) или удалить ветку; сейчас фактическую проверку делает `manager.rs:332-339`. | S |
| F-23 | **low** | весь `crates/core` | **Нет типизированных ошибок** (`anyhow` + строки). Гипотеза «единый тип ошибок (thiserror)» не подтверждена. | Ввести `thiserror`-enum в ядре (коды + человекочитаемые сообщения), маппинг на UI; `anyhow` оставить только на границе. | M |
| F-24 | **low** | `crates/core/src/proxy.rs:122`, `125`; `crates/core/src/qemu.rs:594-596`, `603` | **`unwrap/expect` на пользовательских путях**: паника в fallback'е открытия лога (`File::create(...).unwrap()`), `expect` при создании собственного рантайма. | Заменить на распространение ошибок; fallback писать в `Result`. | S |
| F-25 | **low** | `crates/core/src/store.rs:100-109` | **Битая запись VM молча пропускается** (eprintln) → VM «исчезает» из UI без объяснения. | Собирать список проблем и показывать в UI (как `config_warnings`). | S |
| F-26 | **low** | `crates/core/src/qemu.rs:74-79` | **TOCTOU на свободном порте QMP** (порт освобождается до старта QEMU). | Оставить как есть (retry-цикл `wait_qmp_vnc` покрывает), но задокументировать; либо поднимать QMP на unix-сокете/pipe. | S |
| F-27 | **low** | `crates/core/src/manager.rs:585-602` | **Путь скриншота строится из невалидированного `id`** → возможна запись вне `screenshots/`. | Валидировать `id` (F-02) и санитизировать имя файла. | S |
| F-28 | **low** | `crates/core/src/qemu.rs:107-117` | **Глобальный статический `BINARY_HINT`** — общий для всех экземпляров `Manager`, не сбрасывается между тестами. | Перенести в `Manager`/`Config` (поле), без глобального состояния. | S |
| F-29 | **low** | `crates/core/src/qemu.rs:646-650` | **Fallback клона stdout переоткрывает лог с `truncate(true)`** → лог может быть обрезан прямо во время работы QEMU. | Один `File`, клонировать дескриптор один раз, fallback — открыть с `append`. | S |
| F-30 | **low** | `src-tauri/src/lib.rs:24-29`, `49-57` | **Бесконечный `poll_loop` без механизма остановки**; QEMU не завершается при выходе из приложения (сознательно, но без явной политики и без reattach-подтверждения). | Документировать политику «VM переживают приложение»; добавить в UI явный статус «приложение закрыто, VM работает» и проверку reattach. | S |
| F-31 | **low** | `crates/core/src/store.rs` | **Нет блокировки хранилилища** → два экземпляра приложения пишут один каталог (гонка на `running.json`, дубли VM). | Файл-лок на `base/.lock` (или проверка «уже запущено»). | M |
| F-32 | **low** | `ui/src/lib/api.ts` | **Нет runtime-валидации payload'ов на стороне UI**; типы стираются на IPC-границе. | Валидация на Rust-стороне (главный барьер) + опционально zod на UI. | S |
| F-33 | **low** | `src-tauri/capabilities/default.json:6-9` | `core:default` включает лишние для приложения подмодули (например `core:path`); fs/shell/http — **не** выданы (это хорошо). | Заменить на минимальный явный список (`core:event:default`, `core:window:*` по необходимости, `dialog:*`). | S |
| F-34 | **low** | `.gitignore:5` | Правило `*.log` игнорирует и потенциально нужные тестовые фикстуры; нет `.env*`, `.idea/`, `target/`. | Уточнить правила (`/target`, `/src-tauri/target`, `.env*`, `.idea/`), `*.log` оставить, но оговорить исключение для `tests/fixtures`. | S |
| F-35 | **low** | `src-tauri/Cargo.toml:2`, `package.json:4` | **Моибейк в описании** («Easy QEMU �?») — битый em-dash; в `package.json` нет поля `license` (npm предупреждает). | Починить описания, добавить `"license": "GPL-3.0-only"`. | S |
| F-36 | **low** | `src-tauri/src/commands.rs:182-196` | Label окна `console_<id>` строится из `id` → при нестандартном `id` возможны проблемы Tauri. | Валидировать `id` (F-02) и санитизировать label. | S |
| F-37 | **high** | `.github/workflows/ci.yml:40` | **clippy приложения не может упасть** (`|| true`) → ошибки линтера в `src-tauri` не блокируют PR. | Убрать `|| true`, добавить `-D warnings`. | S |
| F-38 | **high** | `.github/workflows/ci.yml` (целиком) | **CI не покрывает проект:** нет Linux, нет `--workspace`, нет `svelte-check`, нет кэша Rust, нет аудита зависимостей, нет release-workflow, нет Dependabot; сборка `cargo build` вместо `tauri build` (инсталлер не собирается). | Переписать CI (Фаза 1): матрица win+ubuntu, fmt/clippy/test на весь workspace, `npm ci` + `svelte-check` + `npm run build`, кэш Rust/npm, отдельный audit-workflow, release по тегу. | M |
| F-39 | **medium** | `.mcp.json:8`, `.vscode/mcp.json:8` | **Абсолютный локальный путь `D:/Projects/easy-qemu`** в двух файлах → машинно-зависимо, бесполезно для других и для CI. | Убрать из git (в `.gitignore`) или параметризовать. | S |
| F-40 | **low** | `.claude/CLAUDE.md:6-70` | Персональный автогенерируемый блок repowise + **неверные команды** в разделе «Commands» (`npm run build` для Rust-проекта) + дата индексации. | Заменить на вручную написанный `AGENTS.md`; `CLAUDE.md` → ссылка на него. | S |
| F-41 | **low** | `.vscode/extensions.json` | Рекомендация персонального расширения. | Оставить пустым/минимальным. | S |
| F-42 | **low** | `src-tauri/gen/schemas/*.json` (240 КБ) | Сгенерированные схемы в git — допустимо (Tauri v2), но стоит зафиксировать решение и не редактировать вручную. | Оставить, задокументировать. | S |
| F-43 | **low** | `Cargo.toml:7`, `package.json:4`, `ui/package.json:4`, `src-tauri/tauri.conf.json:4` | **Версия продублирована в 4 файлах** → риск рассинхрона на релизе. | Скрипт `scripts/bump-version.mjs` (или `cargo set-version` + правка json), единый источник. | S |
| F-44 | **medium** | `.github/workflows/ci.yml:41-46` | **Артефакт собирается как `cargo build`**, а не `tauri build` → NSIS-инсталлер не проверяется; root-`npm ci` не выполняется → локальный `@tauri-apps/cli` недоступен в CI. | В CI использовать `tauri-action`/`npm run tauri build` с установкой корневых зависимостей. | M |

### Сводка по severity

| Severity | Кол-во | ID |
|---|---|---|
| critical | 1 | F-01 |
| high | 8 | F-02, F-03, F-04, F-05, F-06, F-07, F-37, F-38 |
| medium | 17 | F-08…F-22, F-39, F-44 |
| low | 18 | F-23…F-36, F-40…F-43 |

---

## 5. Приоритизированный план PR (3–5 PR)

Каждый PR — отдельная ветка от `main`, серия атомарных Conventional-коммитов, в конце — отчёт
и ожидание «ок».

### PR-1 · Гигиена репозитория и CI (F-34, F-35, F-37, F-38, F-39, F-40, F-41, F-42, F-43, F-44)
Фундамент: без зелёного CI на двух ОС дальнейшие фазы невозможно проверять.
* `docs/REPO_META.md` (описание/topics/ссылки — применить вручную), README (скриншоты-плейсхолдеры
  в `docs/img`, установка QEMU Win/Linux, быстрый старт, ограничения, FAQ WHPX/KVM/OVMF).
* `CONTRIBUTING.md`, `CHANGELOG.md` (Keep a Changelog), `SECURITY.md`, `.github/ISSUE_TEMPLATE/`,
  `.github/PULL_REQUEST_TEMPLATE.md`, `AGENTS.md` (+ `CLAUDE.md` → ссылка).
* `.gitignore` cleanup, удалить `.mcp.json`/`.vscode/mcp.json` из git, `LICENSE`-поля в package.json,
  чистка моибейка, скрипт бампа версии.
* CI: матрица `windows-latest` + `ubuntu-latest`; `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`; `npm ci` (root+ui), `svelte-check`,
  `npm run build`; кэш Rust (`Swatinem/rust-cache`) и npm; отдельный `audit.yml` (cargo-audit,
  cargo-deny, npm audit); `dependabot.yml` (cargo, npm, github-actions).
* `release.yml` по тегу `v*`: `tauri-action`, NSIS (Windows) + AppImage/deb (Linux), SHA-256
  checksums, черновик релиза из CHANGELOG.
* **Критерий:** CI зелёный на обеих ОС; релиз по тегу собирается без ручных шагов.

### PR-2 · Безопасность: пути, идентификаторы, валидация ввода (F-01, F-02, F-03, F-07, F-08, F-17, F-22, F-27, F-36)
* Новый модуль `crates/core/src/validate.rs` (единая точка валидации): имя VM, id, пути, порты,
  размеры дисков, MAC, tap-имена, snapshot-имена — позитивные/негативные тесты + «fuzz-подобные»
  кейсы (случайные строки, юникод, NUL, разделители, `..`).
* Типизированный builder аргументов QEMU + экранирование запятых в значениях опций.
* Канонизация и проверка принадлежности каталогу дисков при удалении; запрет удаления symlink.
* Устранение `cmd /C` в SSH-лаунчере.
* **Критерий:** на каждую находку — регрессионный тест, который падает до фикса и проходит после;
  формат `vms/*.json` и `config.toml` **не меняется** (совместимость с v0.2).

### PR-3 · Безопасность: консоль, VNC, сеть, Tauri (F-04, F-05, F-06, F-09, F-10, F-19, F-33)
* Токен на сессию консоли + проверка `Origin` + привязка к loopback (уже есть) + закрытие при
  остановке VM (уже есть) — с тестами.
* VNC: рабочая передача пароля в noVNC; предупреждение UI при `vnc_bind` ≠ 127.0.0.1.
* `hostfwd` по умолчанию → 127.0.0.1; 0.0.0.0 — только явным выбором с подтверждением.
* Строгая CSP в `tauri.conf.json`; минимальные capabilities; `pick_path` → async.
* QMP-over-TCP → named pipe (Windows) / unix socket (Linux) с правами 0600 (или, как минимум,
  документированный риск + случайный порт).
* **Критерий:** для каждой находки — регрессионный тест; поведение по умолчанию меняется
  осознанно и фиксируется в CHANGELOG.

### PR-4 · Надёжность и данные (F-11, F-12, F-13, F-14, F-15, F-16, F-25, F-26, F-29, F-31)
* Атомарная запись с `fsync` (файл + каталог) и уникальными tmp-именами; версионирование схемы
  записей (без поломки v0.2: `schema_version` с дефолтом и миграцией).
* Устойчивость `running.json`: битый файл → бэкап + предупреждение, а не тихая потеря.
* Валидация PID по времени старта процесса (F-11); удаление старого pidfile перед стартом + ретраи.
* Ротация/ограничение логов, чтение хвоста, `read_log` → async.
* Персистентное QMP-соединение с reconnect (F-16).
* Файловая блокировка хранилища (F-31); предупреждения о битых записях в UI (F-25).
* **Критерий:** тесты на «убитый процесс во время записи», «битый running.json», «устаревший PID».

### PR-5 · Наблюдаемость и UX-ошибки (F-20, F-21, F-23, F-24, F-28, F-30, F-35, F-43)
* `tracing` + `tracing-subscriber` с файловым выводом и ротацией; уровень в настройках.
* Типизированные ошибки (`thiserror`) с кодами и понятными сообщениями в UI.
* Диагностика поиска бинарей: перечисление проверенных путей, «OVMF не найден», «swtpm не
  поддерживается», статус swtpm в панели окружения.
* Устранение `unwrap/expect` на пользовательских путях; глобальный `BINARY_HINT` → в `Manager`.
* Политика жизненного цикла VM при выходе из приложения (документация + статус в UI).
* **Критерий:** нет `unwrap/expect` в путях, доступных из UI; все ошибки показываются в UI
  с actionable-текстом.

---

## 6. Что подтверждено как «уже хорошо» (не трогать без необходимости)

* Сборка CLI — массив аргументов, без shell нигде в проекте.
* Capabilities минимальны: fs/shell/http **не** выданы.
* Обратная совместимость записей v0.2: `disk_path` → `disks[]` мигрируется в `Vm::normalize()`
  (`vm.rs:732-755`), покрыто тестом `legacy_record_becomes_multi_disk`; `#[serde(default)]`
  на всех новых полях.
* `owned`-семантика дисков: для импортированных дисков `owned=false` (`manager.rs:346`).
* Защита от «убийства чужого процесса» по имени образа (`process.rs:41-58`).
* Остановка прокси консоли при остановке VM и при закрытии окна.
* Переподключение к живому QEMU после перезапуска приложения (running.json + QMP-порт).
* Валидация имён снапшотов в офлайн-пути, валидация MAC, детекция дубликатов hostfwd.
* Тесты ядра: 103 теста, все зелёные; покрыты сборка аргументов, legacy-миграция, удаление
  дисков, round-trip конфига, снапшоты, парсинг статусов.

---

## 7. Приложение: как проверялись гипотезы

* Чтение исходников целиком: `crates/core/src/*.rs` (13 файлов), `src-tauri/src/*.rs`,
  `ui/src/**` (выборочно: `api.ts`, `ConsoleView.svelte`, `VmForm.svelte`, `SettingsModal.svelte`),
  `.github/workflows/ci.yml`, `.claude/CLAUDE.md`, `.mcp.json`, `.vscode/*`, `.gitignore`, `README.md`,
  оба `Cargo.toml`, `package.json`.
* Поиск по дереву: `extra`/`extra_args` (не найдено — функциональности нет), `unwrap|expect|panic`
  (77 совпадений, отфильтрованы тестовые).
* Запуск: `cargo test -p easy-qemu-core` → 74 passed / 0 failed (на момент аудита; в PR-2 — 103).
* Не выполнялось (долгая компиляция таури-зависимостей): полный `cargo clippy --workspace`,
  `cargo test --workspace`, `svelte-check`, `npm run build`, `tauri build`.

---

## 8. Статус исправлений

Таблица обновляется по мере закрытия находок. «Коммит» — хеш в ветке, где находка закрыта.

| ID | Severity | PR | Статус | Коммит | Что сделано |
|---|---|---|---|---|---|
| F-01 | critical | PR-2 | закрыто | `b7c4ebc` | Имя VM проверяется (`validate::vm_name`), путь диска строится через `disk_folder()` + `ensure_inside()`: канонизация и проверка «внутри родителя» |
| F-02 | high | PR-2 | закрыто | `dfd9a76` | `validate::vm_id` — единая точка проверки id; все path builders в `store.rs` возвращают `Result<PathBuf>` |
| F-03 | high | PR-2 | закрыто | `a07324a` | Реестр `<base>/owned-disks.json`: удаляются только зарегистрированные regular-файлы, symlink не удаляется, флаг `owned` вычисляется по реестру, а не берётся из запроса |
| F-07 | high | PR-2 | закрыто | `d5ac5b4` | SSH-клиент запускается напрямую (`ssh.exe` + `CREATE_NEW_CONSOLE`), без `cmd /C`; host/user/port валидируются |
| F-08 | medium | PR-2 | закрыто | `f716828` | Значения QEMU-опций экранируются удвоением запятой; tap-имя, MAC и пути валидируются |
| F-17 | medium | PR-2 | закрыто | `1fc5080` | `size_gb` — диапазон 1..=4096, проверка в `VmDraft::validate` |
| F-18 | medium | PR-2 | закрыто | `3480d3f` | Live-снапшоты используют общий `validate::snapshot_name` (было только в офлайн-пути) |
| F-22 | medium | PR-2 | закрыто | `1fc5080` | Проверка расширения диска реализована: `validate::disk_extension` (qcow2/qcow) |
| F-27 | low | PR-2 | закрыто | `3480d3f` | Id проверяется до построения пути скриншота |
| F-36 | low | PR-2 | закрыто | `dfd9a76` | Label консоли `console_<id>` строится из проверенного id |

Форматы `config.toml`, `vms/*.json` и `running.json` в PR-2 не менялись: совместимость с v0.2
сохранена, добавлен только новый файл `owned-disks.json` рядом с ними.
