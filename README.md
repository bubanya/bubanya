# Bubanya

Лёгкий Minecraft-лаунчер на Tauri 2 + React.

## Возможности

- Vanilla, Fabric и Forge.
- Установка версий, библиотек, assets и нативов.
- Modrinth: поиск модов и модпаков, установка, обновление, переиндексация.
- Импорт `.mrpack` из файла и обновление установленных модпаков.
- Менеджер модов: включение/выключение, удаление, смена версии, drag&drop `.jar`.
- Автоустановка required-зависимостей модов с Modrinth (например, JEI → MezzConfig).
- Ресурспаки, шейдеры и миры: импорт, удаление, бэкап миров.
- Настройки инстанса: память, Java и JVM-аргументы поверх глобальных.
- Java-менеджер: скачивание Adoptium JRE 17/21/25.
- Аккаунты: оффлайн-профили и Microsoft device-code вход.
- Загрузки: прогресс, пауза, продолжение и отмена.
- Логи инстанса прямо в лаунчере.
- Библиотека инстансов: статистика, переименование, дублирование, экспорт/импорт.
- Тёмный минималистичный интерфейс, Silkscreen, сменный акцент.

## Скачать

- Linux AppImage: https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-linux-x86_64.AppImage
- Windows installer: https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-windows-x64-setup.exe
- Windows portable: https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-windows-x64.zip
- Сайт: https://bubanya.github.io/

## Данные

Linux:

```
~/.local/share/Bubanya
```

Windows:

```
%APPDATA%\Bubanya
```

При первом запуске данные старого `OneLauncher` автоматически переносятся в `Bubanya`.

## Сборка

Linux:

```bash
npm install
npm run tauri build -- --bundles appimage
```

Windows:

```powershell
npm install
npm run tauri build -- --bundles nsis
```

Подробности по Windows — в [PORTING_WINDOWS.md](PORTING_WINDOWS.md).

## Разработка

```bash
npm run tauri dev
```

Можно запустить отдельную копию с другим каталогом данных:

```bash
BUBANYA_DATA_DIR=/tmp/bubanya-test npm run tauri dev
```

или `./run.sh` / `.\run.ps1`.
