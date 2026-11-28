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
OUT="dist-windows"
mkdir -p "$OUT"
cp "$EXE" "$OUT/Bubanya.exe"

export BUBANYA_VERSION="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')"
export BUBANYA_PRODUCT="$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["productName"])')"
export BUBANYA_SETUP="$(ls src-tauri/target/$TARGET/release/bundle/nsis/*-setup.exe 2>/dev/null | head -n1 || true)"
if [ -n "$BUBANYA_SETUP" ]; then
  cp "$BUBANYA_SETUP" "$OUT/"
fi

python3 - <<'PY'
import os
from pathlib import Path
import zipfile

out = Path("dist-windows")
version = os.environ["BUBANYA_VERSION"]
product = os.environ["BUBANYA_PRODUCT"]
setup = os.environ.get("BUBANYA_SETUP", "")
setup_name = Path(setup).name if setup else ""

readme = out / "README.txt"
lines = [
    f"{product} {version} для Windows (x64)",
    "",
    "Bubanya.exe — портативная версия.",
]
if setup_name:
    lines.append(f"{setup_name} — установщик.")
lines += [
    "Требуется Microsoft Edge WebView2 Runtime.",
    "Данные: %APPDATA%\\Bubanya",
]
readme.write_text("\n".join(lines) + "\n", encoding="utf-8")

with zipfile.ZipFile(out / f"Bubanya-{version}-win64.zip", "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(out / "Bubanya.exe", "Bubanya.exe")
    archive.write(readme, "README.txt")
print("Готово:", out)
PY
