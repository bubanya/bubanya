#!/usr/bin/env bash
# Кросс-сборка Bubanya под Windows x64 из Linux.
# Требуется: rustup target x86_64-pc-windows-msvc и cargo-xwin в PATH.
# Для NSIS-инсталлятора дополнительно нужен makensis в PATH.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo-xwin >/dev/null 2>&1; then
  echo "cargo-xwin не найден. Скачай prebuilt с https://github.com/rust-cross/cargo-xwin/releases"
  echo "и положи бинарник в ~/.local/bin/cargo-xwin."
  exit 1
fi

if ! rustup target list --installed | grep -qx x86_64-pc-windows-msvc; then
  rustup target add x86_64-pc-windows-msvc
fi

npm run build

TARGET="x86_64-pc-windows-msvc"
BUNDLE_ARGS=(--no-bundle)
if command -v makensis >/dev/null 2>&1; then
  BUNDLE_ARGS=(--config '{"bundle":{"targets":["nsis"]}}')
else
  echo "makensis не найден — собираю только .exe без инсталлятора."
fi

npx tauri build --runner cargo-xwin --target "$TARGET" "${BUNDLE_ARGS[@]}"

EXE="src-tauri/target/$TARGET/release/bubanya.exe"
SETUP="src-tauri/target/$TARGET/release/bundle/nsis/Bubanya_0.1.0_x64-setup.exe"
OUT="dist-windows"
mkdir -p "$OUT"
cp "$EXE" "$OUT/Bubanya.exe"
if [ -f "$SETUP" ]; then
  cp "$SETUP" "$OUT/"
fi

python3 - <<'PY'
from pathlib import Path
import zipfile

out = Path("dist-windows")
readme = out / "README.txt"
if not readme.exists():
    readme.write_text(
        "Bubanya 0.1.0 для Windows (x64)\n\n"
        "Bubanya.exe — портативная версия.\n"
        "Bubanya_0.1.0_x64-setup.exe — установщик (если собран).\n"
        "Требуется Microsoft Edge WebView2 Runtime.\n"
        "Данные: %APPDATA%\\Bubanya\n",
        encoding="utf-8",
    )
with zipfile.ZipFile(out / "Bubanya-0.1.0-win64.zip", "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(out / "Bubanya.exe", "Bubanya.exe")
    archive.write(readme, "README.txt")
print("Готово:", out)
PY
