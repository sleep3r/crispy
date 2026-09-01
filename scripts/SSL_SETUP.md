# 🔐 Настройка SSL сертификата для s3.crispy.fyi

**Кратко:** Let's Encrypt — 90 дней, продлевать вручную или через Cloudflare. Если не хочется возиться — используйте официальный endpoint `s3.ru-3.storage.selcloud.ru` (свой сертификат не нужен).

---

## Вариант 1: Let's Encrypt через DNS Challenge

### Шаг 1: Установка certbot

```bash
# macOS
brew install certbot

# Linux
sudo apt-get update
sudo apt-get install certbot
```

### Шаг 2: Получение сертификата

```bash
# Замени email на свой
certbot certonly --manual --preferred-challenges dns \
  --email your-email@example.com \
  --agree-tos \
  --no-eff-email \
  -d s3.crispy.fyi
```

### Шаг 3: Добавление DNS TXT записи

Certbot попросит добавить TXT запись в DNS:

```
_acme-challenge.s3.crispy.fyi.  TXT  "ваш-токен-от-certbot"
```

**Где добавить:**
- Если домен на Cloudflare: DNS → Records → Add record
- Если на другом провайдере: найдите раздел DNS/DNS Records

**Важно:** После добавления TXT записи подождите 1-2 минуты, затем нажмите Enter в терминале certbot.

### Шаг 4: Копирование сертификата и ключа

После успешной генерации:

```bash
# Certificate (полный цепочка)
sudo cat /etc/letsencrypt/live/s3.crispy.fyi/fullchain.pem

# Private Key
sudo cat /etc/letsencrypt/live/s3.crispy.fyi/privkey.pem
```

### Шаг 5: Добавление в панель управления S3

1. Откройте панель управления S3
2. Перейдите в "Домены" (Domains)
3. Найдите `s3.crispy.fyi`
4. Вставьте:
   - **Certificate**: содержимое `fullchain.pem`
   - **Private Key**: содержимое `privkey.pem`
   - **Country**: RU (или ваш код страны)
   - **Name/Organization**: ваше имя/организация

---

## Вариант 2: Cloudflare SSL (если домен на Cloudflare)

Если домен `crispy.fyi` на Cloudflare:

1. **Включите SSL/TLS** в Cloudflare:
   - Dashboard → SSL/TLS → Overview
   - Выберите "Full" или "Full (strict)"

2. **Настройте CNAME**:
   ```
   s3.crispy.fyi  CNAME  s3.ru-3.storage.selcloud.ru
   ```

3. Cloudflare автоматически предоставит SSL сертификат!

---

## Вариант 3: Использование официального endpoint (быстрое решение)

Если не хотите настраивать кастомный домен прямо сейчас:

```bash
# Используйте официальный endpoint
mc alias set crispy https://s3.ru-3.storage.selcloud.ru \
  <access-key> <secret-key>
```

Это работает сразу без настройки SSL!

---

## Проверка после настройки

```bash
# Проверка SSL
openssl s_client -connect s3.crispy.fyi:443 -servername s3.crispy.fyi

# Тест mc alias
mc ls crispy/
```

---

## ⏰ Let's Encrypt: 90 дней — что делать?

Let's Encrypt выдаёт сертификаты на **90 дней**. Варианты:

### Вариант A: Продлевать вручную (раз в 2–3 месяца)

```bash
# Продление (тот же DNS challenge)
certbot renew --force-renewal

# Новые файлы появятся в тех же путях:
# fullchain.pem, privkey.pem
# Скопируйте их в панель S3 → Домены → обновите сертификат и ключ
```

Настрой напоминание в календаре (например, раз в 2 месяца).

### Вариант B: Cloudflare (рекомендуется, если домен там)

Если домен `crispy.fyi` на **Cloudflare**:
- SSL выдаёт и продлевает **Cloudflare**, не Let's Encrypt
- Срок действия их сертификата — **15 лет**, обновление автоматическое
- В панели S3 тогда нужен сертификат **Origin** от Cloudflare (см. Cloudflare SSL/TLS → Origin Server)

### Вариант C: Вообще без своего домена

Использовать **официальный endpoint** — свой сертификат не нужен, продлевать нечего:

```bash
mc alias set crispy https://s3.ru-3.storage.selcloud.ru <access-key> <secret-key>
```

В коде модели грузить с `https://s3.ru-3.storage.selcloud.ru/crispy/models/...` — всё работает без кастомного домена и без 90-дневного цикла.
