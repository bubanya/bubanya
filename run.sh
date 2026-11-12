#!/usr/bin/env bash
# Запуск Bubanya в режиме разработки (Vite + окно Tauri).
# Для отдельного бинарника без Vite: npm run tauri build -- --debug --no-bundle
set -euo pipefail
cd "$(dirname "$0")"
exec npm run tauri dev
