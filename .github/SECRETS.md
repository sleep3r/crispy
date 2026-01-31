# GitHub Actions: переменные и секреты

Пайплайны сейчас запускаются **без подписи** (`sign-binaries: false`), поэтому для обычной сборки и релиза секреты не нужны.

## Когда включишь подпись (`sign-binaries: true`)

Добавь в **Settings → Secrets and variables → Actions** следующие секреты.

### macOS (подпись + нотаризация)

| Секрет | Описание |
|--------|----------|
| `APPLE_CERTIFICATE` | Сертификат Developer ID Application в base64: `base64 -i certificate.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | Пароль от .p12 |
| `KEYCHAIN_PASSWORD` | Временный пароль для keychain в CI (любая строка) |
| `APPLE_ID` | Apple ID (email) |
| `APPLE_ID_PASSWORD` | Пароль от Apple ID или app-specific password |
| `APPLE_PASSWORD` | То же, что APPLE_ID_PASSWORD (tauri-action использует оба имени) |
| `APPLE_TEAM_ID` | Team ID из Apple Developer (например `XXXXXXXXXX`) |

### Windows (подпись через Tauri / trusted-signing или Azure)

| Секрет | Описание |
|--------|----------|
| `TAURI_SIGNING_PRIVATE_KEY` | Приватный ключ для подписи (если используешь Tauri signing) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Пароль от ключа |
| `AZURE_CLIENT_ID` | Для Azure-based подписи (если используешь) |
| `AZURE_CLIENT_SECRET` | |
| `AZURE_TENANT_ID` | |

### Общее

| Секрет | Описание |
|--------|----------|
| `GITHUB_TOKEN` | Есть по умолчанию в каждом запуске, для релизов и артефактов не настраивать. |

## Какие воркфлоу что делают

- **lint** / **prettier** — только код, секреты не нужны.
- **Build Test** (`build-test.yml`) — ручной запуск, собирает под все платформы, артефакты в run.
- **PR Test Build** (`pr-test-build.yml`) — ручной запуск с номером PR, собирает с ветки PR, комментирует PR ссылкой на артефакты.
- **Release** (`release.yml`) — ручной запуск, создаёт черновик релиза с тегом `v<version>`, собирает под все платформы и загружает артефакты в релиз (без подписи — бинарники будут без подписи/нотаризации).
