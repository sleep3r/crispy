#!/bin/bash
# Collects all diarization-related code into a single prompt file

OUT="diarization_prompt.md"
BASE="$(cd "$(dirname "$0")" && pwd)"

cat > "$OUT" << 'PROMPT_HEADER'
# Задача: Исправить качество диаризации спикеров

## Контекст

Приложение Crispy записывает аудио (через системный микрофон, один канал, все голоса в одном потоке) и транскрибирует его. Опционально включается диаризация — определение, кто из спикеров в какой момент говорит.

## Архитектура пайплайна

1. **Аудио** записывается в WAV (48kHz stereo), при транскрипции ресемплируется в 16kHz mono.
2. **Транскрипция**: аудио нарезается на 30-секундные чанки → каждый отправляется в Parakeet V3 (ONNX модель). При включённой диаризации используется `TimestampGranularity::Word` — каждое слово получает точный `(start, end, text)`.
3. **Сегментация (VAD)**: всё аудио (16kHz mono i16) подаётся в `segmentation-3.0.onnx` (pyannote). Модель обрабатывает 10-секундными окнами и выдаёт Powerset-вероятности (7 классов: silence, spk1, spk2, spk3, spk1+2, spk1+3, spk2+3). Из них формируются `VadSegment` — отрезки аудио, где кто-то говорит.
4. **Эмбеддинги**: каждый `VadSegment` нарезается на чанки ≤3с → подаётся в `wespeaker_en_voxceleb_CAM++.onnx` → получаем вектор-эмбеддинг голоса.
5. **Кластеризация (AHC)**: эмбеддинги кластеризуются Agglomerative Hierarchical Clustering (Average Linkage) с cosine distance. Порог остановки — `threshold` (по умолчанию 0.50). Затем шумовые мини-кластеры фильтруются.
6. **Выравнивание текста**: каждое слово из Parakeet (с точным timestamp) сопоставляется спикеру по midpoint — функция `find_speaker_at_time` ищет, в чей `SpeakerSegment` попадает середина слова.

## Проблема

Диаризация крошит речь одного спикера на фрагменты разных спикеров. Конкретный пример:

Запись `123.wav` (235 секунд, 3 спикера). Первый спикер говорит непрерывно примерно до 38-й секунды. Но результат:
```
Speaker 1 0:00 — "Свой проект по роутерам..."
Speaker 2 0:07 — "Продвинуть бюрократию."
Speaker 3 0:10 — "Вот. А так, в целом..."
Speaker 2 0:17 — "Спасибо"
Speaker 1 0:18 — "за маки, да."
Speaker 3 0:19 — "Никита уже получил..."
```

Всё это на самом деле один спикер до ~38 секунды.

## Диагностика (логи последнего запуска)

```
diarization: threshold=0.6, merge_gap=4, max_speakers=3
segmentation complete: 61 segments found
AHC: 101 embeddings, threshold=0.6, max_speakers=3
distance stats: min=0.0133, p10=0.1197, median=0.6188, p90=0.8381, max=1.0702
AHC stopped at 4 clusters (min_dist=0.6231 > threshold=0.6000)
cluster 0: 17 segments (16.8%)
cluster 1: 58 segments (57.4%)
cluster 2: 26 segments (25.7%)
complete: 3 clusters, 45 merged segments → diarization OK: 45 speaker segments
```

## Корневая причина

Функция `pyannote_get_segments_fixed` создаёт новый `VadSegment` при КАЖДОЙ смене **локальной** метки (spk1→spk2→spk3) внутри 10-секундного окна. Но эти локальные метки **НЕ соответствуют** глобальным спикерам — они просто нумеруют голоса внутри окна. В итоге непрерывная речь одного человека разрезается на 2-3 секундные кусочки, каждый даёт слабый эмбеддинг, и кластеризация назначает их в разные кластеры.

Кроме того, эмбеддинги коротких сегментов (~1-2 секунды) от модели CAM++ очень шумные и ненадёжные.

## Что нужно исправить

1. **Сегментация**: не резать по смене локального спикера — резать ТОЛЬКО по тишине (label=0). Переходы speech→speech (label 1→2→3) не должны создавать новых сегментов. Пусть эмбеддинг + кластеризация определяют, кто говорит.
2. **Минимальная длительность сегмента**: увеличить с 0.3с до ~1.5с. Короткие сегменты дают мусорные эмбеддинги.
3. **Мердж соседних сегментов**: если между двумя speech-сегментами пауза < 0.5с, объединить их ДО вычисления эмбеддинга.
4. **Любые другие улучшения**, которые ты видишь.

## Ограничения

- Весь код на Rust
- Используются зависимости: `pyannote-rs` v0.3, `ort` (ONNX Runtime), `ndarray`, `anyhow`, `transcribe-rs` v0.2.2
- Интерфейс функций `run_diarization` и `format_diarized_text` менять можно, но тогда нужно обновить и вызов в `transcription.rs`
- `EmbeddingExtractor` из `pyannote-rs` принимает `&[i16]` и возвращает iterator of f32

## Что нужно в ответе

Верни ПОЛНЫЙ ИСПРАВЛЕННЫЙ код файла `diarization.rs`. Также верни diff или полный код для `transcription.rs` если интерфейс изменился. Код должен компилироваться. Комментарии по-английски.

---

PROMPT_HEADER

echo "## Файл 1: \`src-tauri/src/managers/diarization.rs\` (основная логика диаризации)" >> "$OUT"
echo "" >> "$OUT"
echo '```rust' >> "$OUT"
cat "$BASE/src-tauri/src/managers/diarization.rs" >> "$OUT"
echo '```' >> "$OUT"
echo "" >> "$OUT"

echo "## Файл 2: \`src-tauri/src/commands/transcription.rs\` (вызывающий код)" >> "$OUT"
echo "" >> "$OUT"
echo '```rust' >> "$OUT"
cat "$BASE/src-tauri/src/commands/transcription.rs" >> "$OUT"
echo '```' >> "$OUT"
echo "" >> "$OUT"

echo "## Файл 3: \`src-tauri/src/managers/transcription.rs\` (TranscriptionManager)" >> "$OUT"
echo "" >> "$OUT"
echo '```rust' >> "$OUT"
cat "$BASE/src-tauri/src/managers/transcription.rs" >> "$OUT"
echo '```' >> "$OUT"
echo "" >> "$OUT"

echo "## Файл 4: \`src-tauri/Cargo.toml\` (зависимости, только релевантные строки)" >> "$OUT"
echo "" >> "$OUT"
echo '```toml' >> "$OUT"
grep -E "pyannote|ort|ndarray|anyhow|transcribe-rs|serde" "$BASE/src-tauri/Cargo.toml" >> "$OUT"
echo '```' >> "$OUT"
echo "" >> "$OUT"

echo "## Файл 5: \`src/components/settings/DiarizationToggle.tsx\` (UI настройки)" >> "$OUT"
echo "" >> "$OUT"
echo '```tsx' >> "$OUT"
cat "$BASE/src/components/settings/DiarizationToggle.tsx" >> "$OUT"
echo '```' >> "$OUT"
echo "" >> "$OUT"

LINES=$(wc -l < "$OUT" | tr -d ' ')
SIZE=$(wc -c < "$OUT" | tr -d ' ')
echo "✅ Создан $OUT: $LINES строк, $(($SIZE / 1024)) КБ"
