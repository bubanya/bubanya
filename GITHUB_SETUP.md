# Публикация Bubanya на GitHub

Аккаунт `bubanya` уже создан. Осталось создать два репозитория и запушить код.

## 1. Создать репозитории

На GitHub:

1. https://github.com/new
   - Owner: `bubanya`
   - Repository name: `bubanya`
   - Public
   - **Не** добавляй README, .gitignore и license.
2. https://github.com/new
   - Owner: `bubanya`
   - Repository name: `bubanya.github.io`
   - Public
   - **Не** добавляй README, .gitignore и license.

## 2. Авторизация git

GitHub не принимает пароль аккаунта для `git push`. Нужен Personal Access Token:

1. GitHub → Settings → Developer settings → Personal access tokens → Tokens (classic).
2. Generate new token (classic).
3. Scopes: `repo` и `workflow`.
4. Скопируй токен.

При первом `git push`:
- Username: `bubanya`
- Password: вставь токен.

Либо добавь SSH-ключ в аккаунт `bubanya` и используй SSH-remote.

## 3. Запушить исходники

```bash
cd /home/ebalai/11/Bubanya
git push -u origin main
```

Проверить, что remote правильный:

```bash
git remote -v
```

Должно быть `https://github.com/bubanya/bubanya.git`.

## 4. Запустить релиз

```bash
cd /home/ebalai/11/Bubanya
git tag v0.1.0
git push origin v0.1.0
```

Тег запускает `.github/workflows/release.yml`:
- Linux AppImage;
- Windows NSIS;
- Windows portable zip;
- GitHub Release.

Перед этим в репозитории `bubanya`:

Settings → Actions → General → Workflow permissions → **Read and write permissions**.

Стабильные ссылки после релиза:

```
https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-linux-x86_64.AppImage
https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-windows-x64-setup.exe
https://github.com/bubanya/bubanya/releases/latest/download/Bubanya-windows-x64.zip
```

## 5. Запушить сайт

```bash
cd /home/ebalai/11/bubanya.github.io
git push -u origin main
```

Затем в репозитории `bubanya.github.io`:

Settings → Pages → Source: **Deploy from a branch** → Branch: `main` / `(root)` → Save.

Сайт: https://bubanya.github.io/

## 6. Если ник заняли

`bubanya` был свободен на момент проверки, но если GitHub откажет:

- используй `bubanya-launcher` и поменяй ссылки в сайте и workflow;
- или сделай project page у существующего аккаунта: `asswwa.github.io/bubanya`.

## Обновление версии

1. Подними версию в `package.json` и `src-tauri/tauri.conf.json`.
2. Закоммить изменения.
3. Поставь новый тег:
   ```bash
   git tag v0.1.1
   git push origin v0.1.1
   ```
4. Workflow соберёт новый релиз, ссылки `latest/download/...` останутся прежними.
