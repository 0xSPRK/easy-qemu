# Easy QEMU

Кроссплатформенный GUI-менеджер виртуальных машин QEMU (Windows в приоритете, Linux поддерживается).

Tauri 2 + Svelte 5 · ядро на Rust (`crates/core`) · консоль ВМ — встроенный noVNC.

## Возможности

- Создание/редактирование/удаление ВМ (qcow2, ISO, BIOS/UEFI-OVMF, q35/i440fx/microvm)
- Запуск headless / VNC / GTK / SDL, автоподбор акселератора (WHPX/KVM/TCG)
- Управление через QMP: пауза, продолжение, сброс, ACPI-выключение, force-stop
- Консоль в отдельном окне через noVNC (WebSocket-прокси на 127.0.0.1)
- Проброс портов для user-NET (hostfwd tcp/udp)
- Снапшоты qcow2 (qemu-img snapshot)
- Просмотр лога QEMU по каждой ВМ; безопасное удаление только «своих» дисков

## Структура

```
crates/core   ядро без UI: store/config/qemu/qmp/proxy/snapshots/manager
src-tauri     клей Tauri: команды, события статусов, окна консоли
ui            фронтенд (Vite + Svelte 5 + TS), noVNC бандлится npm-ом
```

## Разработка

Требования: Rust (MSVC), Node 20+, WebView2 Runtime (есть в Win10/11).

```bash
npm install --prefix ui
cargo test -p easy-qemu-core      # тесты ядра
npm run tauri dev                 # запуск в режиме разработки
```

## Сборка

```bash
npm run tauri build               # NSIS-инсталлятор в src-tauri/target/release/bundle/
```

## Расположение данных

- Конфиг: `%APPDATA%/easy-qemu/config.toml` (Linux: `~/.config/easy-qemu`)
- Записи ВМ: `<config>/vms/*.json`, состояние: `running.json`
- Диски и логи: каталог из настроек (по умолчанию `<config>/disks`)

## Лицензия

MIT
