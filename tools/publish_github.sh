#!/usr/bin/env bash
# Публикация Bubanya на GitHub.
# Перед запуском создай репозитории bubanya/bubanya и bubanya/bubanya.github.io.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE="$ROOT/../bubanya.github.io"
TAG="${1:-v0.1.0}"

# Токен кэшируется в памяти на час, в файлы не пишется.
GIT_CRED=(-c "credential.helper=cache --timeout=3600")

echo "== Исходники: $ROOT"
cd "$ROOT"
git "${GIT_CRED[@]}" push -u origin main

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Тег $TAG уже существует, пушу его."
else
  git tag "$TAG"
fi
git "${GIT_CRED[@]}" push origin "$TAG"

if [ -d "$SITE/.git" ]; then
  echo "== Сайт: $SITE"
  cd "$SITE"
  git "${GIT_CRED[@]}" push -u origin main
else
  echo "Сайт не найден: $SITE"
fi

echo
echo "Готово. Проверь:"
echo "  https://github.com/bubanya/bubanya"
echo "  https://github.com/bubanya/bubanya/releases"
echo "  https://bubanya.github.io/"
