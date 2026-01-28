# Исправление: Плагин не появляется в системе

## Проблема
Плагин установлен, но "Crispy Microphone" не появляется в списке входных устройств macOS.

## Причина
1. **Плагин не подписан** - macOS требует подпись для всех HAL плагинов
2. **CoreAudio не перезапущен** - из-за SIP (System Integrity Protection) автоматический перезапуск не работает

## Решение

### Шаг 1: Пересобрать плагин с подписью

```bash
cd macos/virtual-mic
make clean
make
```

Это создаст подписанный плагин в `target/CrispyVirtualMic.driver`.

### Шаг 2: Переустановить плагин

```bash
# Удалить старый
sudo rm -rf /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver

# Установить новый (из target/)
sudo cp -R target/CrispyVirtualMic.driver /Library/Audio/Plug-Ins/HAL/

# Подписать установленный плагин
sudo codesign --sign - --force --deep /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver
```

### Шаг 3: Перезапустить CoreAudio

**Вариант 1 (рекомендуется):**
```bash
sudo killall coreaudiod
```

**Вариант 2 (если первый не работает):**
1. Откройте **Терминал**
2. Выполните: `sudo launchctl kickstart -k system/com.apple.audio.coreaudiod`
3. Введите пароль администратора

**Вариант 3 (если ничего не помогает):**
Перезагрузите Mac.

### Шаг 4: Проверить установку

```bash
# Проверить, что плагин установлен
ls -la /Library/Audio/Plug-Ins/HAL/ | grep Crispy

# Проверить подпись
codesign -vv /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver/Contents/MacOS/CrispyVirtualMic

# Проверить в системе
system_profiler SPAudioDataType | grep -i crispy
```

### Шаг 5: Проверить в настройках

1. Откройте **Системные настройки** → **Звук** → **Вход** (Input)
2. Должен появиться **"Crispy Microphone"**

Или откройте **Audio MIDI Setup** (в Программы → Утилиты):
- Найдите "Crispy Microphone" в списке устройств
- Должна быть видна активность при записи

## Автоматическая установка (обновлено)

Теперь Makefile автоматически подписывает плагин:

```bash
make plugin-install
```

Но CoreAudio всё равно нужно перезапустить вручную из-за SIP.

## Отладка

Если плагин всё ещё не появляется:

1. **Проверить логи CoreAudio:**
   ```bash
   log show --predicate 'subsystem == "com.apple.audio"' --last 10m | grep -i "crispy\|hal\|error"
   ```

2. **Проверить, что плагин загружается:**
   ```bash
   sudo dtruss -p $(pgrep coreaudiod) 2>&1 | grep -i crispy
   ```

3. **Проверить права доступа:**
   ```bash
   ls -la /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver
   ```

4. **Проверить архитектуру:**
   ```bash
   file /Library/Audio/Plug-Ins/HAL/CrispyVirtualMic.driver/Contents/MacOS/CrispyVirtualMic
   ```
   Должно быть: `Mach-O 64-bit bundle x86_64` (или `arm64` на Apple Silicon)

## Примечания

- **Ad-hoc подпись (`-`)** работает для разработки, но для распространения нужен Developer ID
- **SIP (System Integrity Protection)** может блокировать автоматический перезапуск CoreAudio
- Плагин работает на **x86_64** (через Rosetta на Apple Silicon) из-за Rust toolchain
- После установки может потребоваться **несколько секунд** для появления устройства
