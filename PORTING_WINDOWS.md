# Портирование Bubanya на Windows

Статус: начато. Linux-версия собрана и установлена, ниже — что уже сделано и что осталось.

## Уже сделано

- Tauri 2 + React-фронтенд не содержат Linux-only UI.
- Java-детект расширен под Windows:
  - `JAVA_HOME`, `PATH`;
  - `%ProgramFiles%\Java`, `Eclipse Adoptium`, `Eclipse Foundation`, `Microsoft`, `Zulu`, `BellSoft`, `Amazon Corretto`;
  - `%LOCALAPPDATA%\Programs\Eclipse Adoptium`;
  - рантаймы PrismLauncher в `%APPDATA%` и `%LOCALAPPDATA%`.
- Refresh-токены Microsoft на Windows хранятся в **Windows Credential Manager** через crate `keyring` (на Linux — GNOME Keyring/`secret-tool`).
- Экспорт/импорт и тосты корректно разбирают и `\`, и `/` в путях.
- Forge-установщик на Windows работает без Linux-обхода zlib; ветка `LD_PRELOAD` включается только на Linux.
- Игра запускается через `javaw.exe` (без лишнего консольного окна), а установщик Forge и проверка Java — через `java.exe` с `CREATE_NO_WINDOW`.
- `run.ps1` для dev-запуска и `tools/build_windows.ps1` для сборки NSIS-инсталлятора.
- Добавлен GitHub Actions workflow `.github/workflows/windows.yml` для сборки NSIS на `windows-latest`.

## Требования для сборки на Windows

1. **Rust** с toolchain `stable-x86_64-pc-windows-msvc`.
2. **Visual Studio Build Tools** с компонентом «Desktop development with C++» (MSVC + Windows SDK).
3. **Node.js 20+** и npm.
4. **WebView2 Runtime** (на Windows 10/11 обычно уже установлен; Tauri может поставить bootstrapper).
5. Для NSIS-инсталлятора Tauri сам скачает NSIS; для MSI нужен WiX.

## Сборка на Windows

```powershell
cd Bubanya
npm install
npm run tauri build -- --bundles nsis
```

Результат:

```
src-tauri\target\release\bundle\nsis\Bubanya_0.1.0_x64-setup.exe
```

Либо просто `npm run tauri build` — Tauri соберёт все доступные bundle-форматы.

## Кросс-сборка из Linux (экспериментально)

Собрать `.exe` и NSIS-инсталлятор можно без Windows-машины через `cargo-xwin`
(MSVC-таргет) и Linux-сборку `makensis`:

```bash
rustup target add x86_64-pc-windows-msvc

# cargo-xwin: prebuilt с https://github.com/rust-cross/cargo-xwin/releases
# (x86_64-unknown-linux-musl.tar.gz, бинарник в ~/.local/bin/cargo-xwin)

# makensis: например, распакованные пакеты nsis и nsis-common из Debian
# (makensis должен быть в PATH; NSISDIR внутри обёртки)

cd Bubanya
npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc \
  --config '{"bundle":{"targets":["nsis"]}}'
```

Результат:

```
src-tauri/target/x86_64-pc-windows-msvc/release/bubanya.exe
src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/Bubanya_0.1.0_x64-setup.exe
```

Без `makensis` используй `--no-bundle` — соберётся только `.exe`.

Готовый скрипт: `tools/build_windows_linux.sh` — собирает exe, при наличии
`makensis` ещё и инсталлятор, и кладёт всё в `dist-windows/`
(`Bubanya-0.1.0-win64.zip` — портативная версия).

Особенности:

- Tauri CLI на Linux принимает NSIS только через `--config`, флаг
  `--bundles nsis` отфильтрован по хосту.
- `makensis` вызывается из PATH. Если NSIS распакован в нестандартное место,
  нужна обёртка, которая выставляет `NSISDIR` (Tauri сам удаляет эту переменную).
- Предупреждения `lld-link: Cannot use debug info for 'libcmt.lib...'` безвредны:
  в MSVC-пакете нет PDB для системных библиотек.
- WebView2Loader линкуется статически, отдельная DLL не нужна. На целевой
  машине должен быть установлен WebView2 Runtime.
- Экзешник и инсталлятор не подписаны: SmartScreen может ругаться.

## Запуск в разработке

```powershell
.\run.ps1
```

или:

```powershell
npm run tauri dev
```

## Данные приложения на Windows

```
%APPDATA%\Bubanya\
```

Там же: `instances.json`, `accounts.json`, `settings.json`, `meta\`, `instances\`.

## Что ещё нужно проверить/доделать

- [x] Собрать `.exe` кросс-сборкой из Linux (cargo-xwin, MSVC).
- [ ] Запустить `.exe` на реальной Windows 10/11.
- [ ] Проверить Java-детект на чистой системе без PrismLauncher.
- [ ] Проверить Microsoft device-code вход и Windows Credential Manager.
- [ ] Проверить Forge-инсталлер на Windows (без zlib-обхода).
- [ ] Проверить пути с пробелами/кириллицей (`C:\Users\...\AppData\...`).
- [ ] Проверить запуск Minecraft с пробелами в пути к Java/игре.
- [ ] Добавить GitHub Actions matrix (Linux + Windows) для сборки.
- [ ] Проверить иконки и ярлык в меню «Пуск».
- [ ] Проверить `closeOnLaunch`, логи, нативы LWJGL на Windows.
- [ ] Опционально: macOS-порт (сейчас не начат).

## Известные ограничения

- Microsoft-токены на Linux хранятся в keyring; если keyring недоступен — вход живёт до перезапуска. На Windows используется Credential Manager.
- RAR-архивы по-прежнему не импортируются; поддерживаются `.zip` (Bubanya/Prism).
- Forge-версии тестировались на 1.20.1; для старых Forge может понадобиться отдельная обработка `minecraftArguments`.
