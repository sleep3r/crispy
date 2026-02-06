# Исправление Speaker Diarization

## Проблема

**Speaker diarization возвращала 0 сегментов** для всех аудио файлов из-за **двух багов в `pyannote-rs` v0.3**:

1. ❌ **Итератор завершался рано** - после первого 10-сек окна если не найдено сегментов
2. ❌ **Неправильная нормализация** - `i16 as f32` вместо деления на 32768.0
   - Модель получала значения ±15000 вместо ±0.5
   - Это приводило к "degenerate outputs" (весь audio = class 0)
3. ❌ **Неправильный формат входных данных** - подавали 48kHz вместо 16kHz

## Решение

### 1. Заменили `pyannote_rs::get_segments()` на исправленную версию

Создали функцию `pyannote_get_segments_fixed()` в `diarization.rs`:
- ✅ Правильная нормализация: `i16 / 32768.0` → `[-1, 1]`
- ✅ Обрабатывает **все** 10-секундные окна (не завершается рано)
- ✅ Flush'ит финальный открытый сегмент в конце
- ✅ Требует 16kHz mono (enforced)

### 2. Используем правильные audio samples

**До:**
```rust
// ❌ Использовали pyannote_rs::read_wav() - возвращает 48kHz
let (samples, sr) = pyannote_rs::read_wav(&recording_path)?;
run_diarization(&samples, sr, ...); // 48000 Hz - WRONG!
```

**После:**
```rust
// ✅ Используем уже готовые 16kHz mono samples из transcription pipeline
let samples_i16 = f32_to_i16(&all_audio_16k); // Vec<f32> → Vec<i16>
run_diarization(&samples_i16, 16000, ...);    // 16000 Hz - CORRECT!
```

### 3. Добавили валидацию

```rust
pub fn run_diarization(..., sample_rate: u32, ...) -> Result<...> {
    // Enforce 16kHz requirement
    if sample_rate != 16_000 {
        bail!("Diarization requires 16kHz mono; got {} Hz", sample_rate);
    }
    // ...
}
```

## Что изменилось

### Файл: `src-tauri/src/managers/diarization.rs`

1. **Добавлены импорты**:
   ```rust
   use anyhow::{bail, Context};
   use ndarray::{Array1, Axis, IxDyn};
   use ort::{session::Session, value::TensorRef};
   ```

2. **Добавлена структура `VadSegment`**:
   ```rust
   struct VadSegment {
       start: f64,
       end: f64,
       samples: Vec<i16>,
   }
   ```

3. **Добавлена функция `pyannote_get_segments_fixed()`** (~150 строк):
   - Загружает ONNX model напрямую через `ort`
   - Обрабатывает аудио окнами по 10 секунд (160,000 samples @ 16kHz)
   - Правильная нормализация: `i16 / 32768.0`
   - Voice Activity Detection через argmax по классам
   - Возвращает `Vec<VadSegment>` вместо Iterator

4. **Обновлена `run_diarization()`**:
   - Добавлена проверка `sample_rate == 16000`
   - Заменён вызов `pyannote_rs::get_segments()` на `pyannote_get_segments_fixed()`
   - Упрощён цикл обработки (Vec вместо Iterator<Result>)

### Файл: `src-tauri/src/commands/transcription.rs`

**Удалён fallback на `pyannote_rs::read_wav()`**:
```diff
- eprintln!("[transcription] trying pyannote_rs::read_wav on source file...");
- let (native_samples, native_sr) = match pyannote_rs::read_wav(recording_path) {
-     Ok(v) => v,
-     Err(e) => (Vec::new(), 0)
- };
- let (samples_i16, sr) = if !native_samples.is_empty() {
-     (native_samples, native_sr)  // ❌ 48kHz
- } else {
-     (f32_to_i16(&all_audio_16k), 16000)  // ✅ 16kHz
- };

+ // Convert resampled f32 audio to i16 for diarization
+ let samples_i16 = f32_to_i16(&all_audio_16k);
+ let sr = 16000;  // ✅ Always 16kHz
```

## Тестирование

Запусти:
```bash
make dev
```

1. Включи diarization в Settings → Transcription
2. Транскрибируй любую запись с речью (лучше диалог)
3. Проверь логи в консоли:

**Ожидаемые логи:**
```
[diarization] input: 5524992 samples, 16000Hz, 345.3s, ...
[diarization] using fixed segmentation (bypasses pyannote-rs bugs)
[diarization] pyannote_get_segments_fixed: starting segmentation
[diarization] processing 35 windows of 10s each
[diarization] segment found: 2.15s - 8.42s (100160 samples)
[diarization] segment found: 10.22s - 15.88s (90560 samples)
...
[diarization] pyannote_get_segments_fixed: 42 segments found  ✅
[diarization] segment #1: start=2.15s end=8.42s samples=100160
[diarization] segment #1 -> Speaker 1
[diarization] segment #2: start=10.22s end=15.88s samples=90560
[diarization] segment #2 -> Speaker 2
...
```

4. Открой результат транскрипции - должны быть **цветные блоки с метками спикеров**:
   ```
   [Speaker 1]
   Привет, как дела?
   
   [Speaker 2]
   Отлично, спасибо!
   
   [Speaker 1]
   ...
   ```

## Почему это работает

### Корневая причина баги в pyannote-rs

**Баг #1: Iterator terminates early**
```rust
// pyannote-rs source (BUGGY):
std::iter::from_fn(move || {
    if let Some(segment) = queue.pop_front() {
        return Some(Ok(segment));
    }
    None  // ❌ Возвращает None если queue пустая, заканчивает итерацию навсегда!
})
```

Если первое 10-сек окно не содержит полного "speech → non-speech" перехода, queue остаётся пустой → `None` → цикл `for segment in segments` выполняется **0 раз**.

**Баг #2: Wrong normalization**
```rust
// pyannote-rs source (BUGGY):
let samples_f32: Vec<f32> = samples_i16.iter()
    .map(|&x| x as f32)  // ❌ -15000..+15000 вместо -1..+1
    .collect();
```

Модель ожидает `[-1, 1]`, получает `[-15000, +15000]` → всё предсказывает как class 0 (silence).

### Наше решение

```rust
// Правильная нормализация:
let window_f32: Vec<f32> = window_i16.iter()
    .map(|&x| x as f32 / 32768.0)  // ✅ -1..+1
    .collect();

// Обрабатываем ВСЕ окна:
while win_start < padded.len() {
    // process window
    win_start += window_size;  // ✅ Продолжаем независимо от результата
}
```

## Дополнительная информация

- **pyannote.audio models** тренируются на **16kHz mono**
- **Segmentation model** (`segmentation-3.0.onnx`) выполняет Voice Activity Detection
- **Embedding model** (`wespeaker_en_voxceleb_CAM++.onnx`) создаёт speaker embeddings
- **Language-independent**: segmentation работает на любом языке (включая русский)

## Ссылки

- [pyannote-rs source](https://docs.rs/pyannote-rs/latest/src/pyannote_rs/segment.rs.html)
- [pyannote.audio pipeline](https://huggingface.co/pyannote/speaker-diarization-3.0)
- [Reference implementation](https://gist.github.com/thewh1teagle/b3f1002c690ac567b4cef0613e0fbfa8)

---

**Статус**: ✅ ИСПРАВЛЕНО
**Дата**: 2026-02-06
