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

## Файл 1: `src-tauri/src/managers/diarization.rs` (основная логика диаризации)

```rust
// Speaker diarization using pyannote-rs.
// Two ONNX models: segmentation + speaker embedding.
//
// Improvements over naive approach:
// - Proper Powerset decoding for segmentation-3.0
// - Median filter to remove micro-glitches
// - Chunking long segments for better embeddings
// - Agglomerative Hierarchical Clustering (AHC) instead of greedy online matching
// - Overlap-based text alignment instead of point-in-time matching

use anyhow::{bail, Context, Result};
use log::info;
use ndarray::{Array1, Axis, IxDyn};
use ort::{session::Session, value::TensorRef};
use pyannote_rs::EmbeddingExtractor;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpeakerSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

#[derive(Debug, Clone)]
struct VadSegment {
    start: f64,
    end: f64,
    samples: Vec<i16>,
}

/// Improved VAD segmentation via pyannote segmentation-3.0
/// Decodes Powerset probabilities and correctly splits audio on speaker changes.
fn pyannote_get_segments_fixed(
    samples: &[i16],
    sample_rate: u32,
    segmentation_model_path: &std::path::Path,
) -> Result<Vec<VadSegment>> {
    if sample_rate != 16_000 {
        bail!("pyannote segmentation expects 16kHz mono. Got {} Hz.", sample_rate);
    }
    if samples.is_empty() {
        return Ok(vec![]);
    }

    eprintln!("[diarization] starting advanced Powerset segmentation");

    let mut session = Session::builder()
        .context("ort: Session::builder failed")?
        .commit_from_file(segmentation_model_path)?;

    let frame_step: usize = 270;
    let frame_start: usize = 721;
    let window_size: usize = (sample_rate as usize) * 10; // 10 seconds

    let mut padded: Vec<i16> = Vec::with_capacity(samples.len() + window_size * 2);
    padded.extend_from_slice(samples);
    let rem = padded.len() % window_size;
    if rem != 0 {
        padded.extend(std::iter::repeat(0i16).take(window_size - rem));
    }
    padded.extend(std::iter::repeat(0i16).take(window_size)); // Extra window for trailing speech

    let mut out: Vec<VadSegment> = Vec::new();
    let mut win_start = 0usize;

    while win_start < padded.len() {
        let win_end = win_start + window_size;
        let window_i16 = &padded[win_start..win_end];

        let mut window_f32 = vec![0f32; window_size];
        for (src, dst) in window_i16.iter().zip(window_f32.iter_mut()) {
            *dst = *src as f32 / 32768.0;
        }

        let input = Array1::from(window_f32)
            .insert_axis(Axis(0))
            .insert_axis(Axis(1));
        let outputs = session
            .run(ort::inputs![TensorRef::from_array_view(input.view().into_dyn())?])?;

        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        let shape_vec: Vec<usize> = (0..shape.len()).map(|i| shape[i] as usize).collect();
        let view =
            ndarray::ArrayViewD::<f32>::from_shape(IxDyn(&shape_vec), data)?;

        let frames = if shape_vec.len() == 3 {
            view.index_axis(Axis(0), 0)
        } else {
            view
        };
        let classes = frames.shape()[1];
        let mut local_labels = Vec::with_capacity(frames.shape()[0]);

        // 1. Decode Powerset into marginal speaker probabilities
        for probs in frames.axis_iter(Axis(0)) {
            let max_val = probs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mut exps = vec![0.0; classes];
            let mut sum_exp = 0.0;
            for (i, &v) in probs.iter().enumerate() {
                let e = (v - max_val).exp();
                exps[i] = e;
                sum_exp += e;
            }
            for e in exps.iter_mut() {
                *e /= sum_exp;
            }

            let (p_sil, p_s1, p_s2, p_s3) = if classes == 7 {
                // Powerset: [silence, spk1, spk2, spk3, spk1+2, spk1+3, spk2+3]
                (
                    exps[0],
                    exps[1] + exps[4] + exps[5],
                    exps[2] + exps[4] + exps[6],
                    exps[3] + exps[5] + exps[6],
                )
            } else if classes >= 4 {
                (exps[0], exps[1], exps[2], exps[3])
            } else {
                (exps[0], exps.get(1).copied().unwrap_or(0.0), 0.0, 0.0)
            };

            let label = if p_sil > 0.5 {
                0
            } else if p_s1 >= p_s2 && p_s1 >= p_s3 {
                1
            } else if p_s2 >= p_s1 && p_s2 >= p_s3 {
                2
            } else {
                3
            };

            local_labels.push(label);
        }

        // 2. Median filter (~180ms) to remove micro-glitches
        let window_len = 11;
        let half = window_len / 2;
        let mut smoothed = vec![0u8; local_labels.len()];
        for i in 0..local_labels.len() {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(local_labels.len());
            let mut counts = [0usize; 4];
            for &l in &local_labels[start..end] {
                counts[l as usize] += 1;
            }
            let mut max_c = 0;
            let mut max_l = 0;
            for (l, &c) in counts.iter().enumerate() {
                if c > max_c {
                    max_c = c;
                    max_l = l as u8;
                }
            }
            smoothed[i] = max_l;
        }

        // 3. Build segments — cut on speaker change, not just on silence
        let mut current_label = 0u8;
        let mut start_frame = 0usize;

        let push_segment =
            |label: u8, start_f: usize, end_f: usize, out_vec: &mut Vec<VadSegment>| {
                if label != 0 {
                    let start_idx =
                        (win_start + frame_start + start_f * frame_step).min(samples.len());
                    let end_idx =
                        (win_start + frame_start + end_f * frame_step).min(samples.len());

                    // Ignore micro-breaths shorter than 0.3s (they produce garbage embeddings)
                    if end_idx.saturating_sub(start_idx) >= 4800 {
                        out_vec.push(VadSegment {
                            start: start_idx as f64 / sample_rate as f64,
                            end: end_idx as f64 / sample_rate as f64,
                            samples: samples[start_idx..end_idx].to_vec(),
                        });
                    }
                }
            };

        for (i, &label) in smoothed.iter().enumerate() {
            if label != current_label {
                push_segment(current_label, start_frame, i, &mut out);
                current_label = label;
                start_frame = i;
            }
        }
        push_segment(current_label, start_frame, smoothed.len(), &mut out);
        win_start += window_size;
    }

    eprintln!(
        "[diarization] segmentation complete: {} segments found",
        out.len()
    );

    Ok(out)
}

/// Run speaker diarization on 16 kHz mono i16 samples.
/// Uses Agglomerative Hierarchical Clustering (AHC) instead of greedy online matching.
pub fn run_diarization(
    samples_i16: &[i16],
    sample_rate: u32,
    segmentation_model_path: &PathBuf,
    embedding_model_path: &PathBuf,
    max_speakers: usize,
    threshold: f64,
    merge_gap: f64,
) -> Result<Vec<SpeakerSegment>> {
    if sample_rate != 16_000 {
        bail!("Requires 16kHz mono.");
    }

    let duration_secs = samples_i16.len() as f64 / sample_rate as f64;
    eprintln!(
        "[diarization] input: {} samples, {}Hz, {:.1}s",
        samples_i16.len(),
        sample_rate,
        duration_secs
    );

    let segments =
        pyannote_get_segments_fixed(samples_i16, sample_rate, segmentation_model_path)?;
    if segments.is_empty() {
        return Ok(Vec::new());
    }

    let mut extractor =
        EmbeddingExtractor::new(embedding_model_path.to_str().unwrap_or(""))
            .map_err(|e| anyhow::anyhow!("Failed to load embedding model: {:?}", e))?;

    // Chunk long monologues: ResNet34 loses accuracy on segments > 3-4 seconds.
    let mut chunked_segments = Vec::new();
    let max_dur = 3.0;

    for seg in segments {
        let dur = seg.end - seg.start;
        if dur > max_dur {
            let chunks = (dur / max_dur).ceil() as usize;
            let chunk_samples = seg.samples.len() / chunks;
            for i in 0..chunks {
                let start_idx = i * chunk_samples;
                let end_idx = if i == chunks - 1 {
                    seg.samples.len()
                } else {
                    (i + 1) * chunk_samples
                };
                chunked_segments.push(VadSegment {
                    start: seg.start + (start_idx as f64 / sample_rate as f64),
                    end: seg.start + (end_idx as f64 / sample_rate as f64),
                    samples: seg.samples[start_idx..end_idx].to_vec(),
                });
            }
        } else {
            chunked_segments.push(seg);
        }
    }

    let mut valid_embeddings = Vec::new();
    let mut valid_segments = Vec::new();

    for segment in chunked_segments {
        if let Ok(embedding) = extractor.compute(&segment.samples) {
            valid_embeddings.push(embedding.collect::<Vec<f32>>());
            valid_segments.push(segment);
        }
    }

    if valid_segments.is_empty() {
        return Ok(Vec::new());
    }

    // Agglomerative Hierarchical Clustering (Average Linkage)
    let n = valid_embeddings.len();
    eprintln!(
        "[diarization] AHC: {} embeddings, threshold={}, max_speakers={}",
        n, threshold, max_speakers
    );
    let mut clusters: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut dist_matrix = vec![vec![0.0f32; n]; n];

    for i in 0..n {
        for j in (i + 1)..n {
            let dist = cosine_distance(&valid_embeddings[i], &valid_embeddings[j]);
            dist_matrix[i][j] = dist;
            dist_matrix[j][i] = dist;
        }
    }

    // Log distance statistics for debugging
    {
        let mut all_dists: Vec<f32> = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                all_dists.push(dist_matrix[i][j]);
            }
        }
        if !all_dists.is_empty() {
            all_dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = all_dists[all_dists.len() / 2];
            let p10 = all_dists[all_dists.len() / 10];
            let p90 = all_dists[all_dists.len() * 9 / 10];
            eprintln!(
                "[diarization] distance stats: min={:.4}, p10={:.4}, median={:.4}, p90={:.4}, max={:.4}",
                all_dists[0], p10, median, p90, all_dists[all_dists.len() - 1]
            );
        }
    }

    // Phase 1: Merge clusters while min distance is below threshold (natural speaker boundaries)
    loop {
        if clusters.len() <= 1 {
            break;
        }

        let mut min_dist = f32::MAX;
        let mut merge_pair = (0, 0);
        let k = clusters.len();

        for i in 0..k {
            for j in (i + 1)..k {
                let mut sum_dist = 0.0;
                let cl_i = &clusters[i];
                let cl_j = &clusters[j];
                for &u in cl_i {
                    for &v in cl_j {
                        sum_dist += dist_matrix[u][v];
                    }
                }
                let avg_dist = sum_dist / (cl_i.len() * cl_j.len()) as f32;
                if avg_dist < min_dist {
                    min_dist = avg_dist;
                    merge_pair = (i, j);
                }
            }
        }

        // Stop merging when distance exceeds threshold — these are different speakers
        if min_dist > threshold as f32 {
            eprintln!(
                "[diarization] AHC stopped at {} clusters (min_dist={:.4} > threshold={:.4})",
                clusters.len(), min_dist, threshold
            );
            break;
        }

        let (i, j) = merge_pair;
        let mut merged = clusters[i].clone();
        merged.extend(clusters[j].iter().copied());
        clusters.remove(j); // j is always > i
        clusters.remove(i);
        clusters.push(merged);
    }

    // Phase 2: Filter out noise clusters (tiny clusters with < 2% of segments)
    // and reassign their segments to the nearest real cluster.
    let min_cluster_size = (n as f64 * 0.02).ceil() as usize;
    let min_cluster_size = min_cluster_size.max(2); // at least 2 segments
    eprintln!(
        "[diarization] Phase 2: {} clusters after threshold, min_cluster_size={}",
        clusters.len(), min_cluster_size
    );

    // Separate real clusters from noise
    let mut real_clusters: Vec<Vec<usize>> = Vec::new();
    let mut noise_indices: Vec<usize> = Vec::new();
    for cluster in &clusters {
        if cluster.len() >= min_cluster_size {
            real_clusters.push(cluster.clone());
        } else {
            noise_indices.extend(cluster.iter());
        }
    }

    // If no real clusters, keep the largest one
    if real_clusters.is_empty() {
        clusters.sort_by(|a, b| b.len().cmp(&a.len()));
        real_clusters.push(clusters[0].clone());
        noise_indices.clear();
        for cluster in clusters.iter().skip(1) {
            noise_indices.extend(cluster.iter());
        }
    }

    // Reassign noise segments to the nearest real cluster
    for &idx in &noise_indices {
        let mut best_cluster = 0;
        let mut best_dist = f32::MAX;
        for (ci, cluster) in real_clusters.iter().enumerate() {
            let avg_dist: f32 = cluster.iter()
                .map(|&c| dist_matrix[idx][c])
                .sum::<f32>() / cluster.len() as f32;
            if avg_dist < best_dist {
                best_dist = avg_dist;
                best_cluster = ci;
            }
        }
        real_clusters[best_cluster].push(idx);
    }

    // If still more clusters than max_speakers, merge closest pairs
    while real_clusters.len() > max_speakers {
        let mut min_dist = f32::MAX;
        let mut merge_pair = (0, 0);
        let k = real_clusters.len();
        for i in 0..k {
            for j in (i + 1)..k {
                let mut sum_dist = 0.0;
                for &u in &real_clusters[i] {
                    for &v in &real_clusters[j] {
                        sum_dist += dist_matrix[u][v];
                    }
                }
                let avg_dist = sum_dist / (real_clusters[i].len() * real_clusters[j].len()) as f32;
                if avg_dist < min_dist {
                    min_dist = avg_dist;
                    merge_pair = (i, j);
                }
            }
        }
        let (i, j) = merge_pair;
        let mut merged = real_clusters[i].clone();
        merged.extend(real_clusters[j].iter().copied());
        real_clusters.remove(j);
        real_clusters.remove(i);
        real_clusters.push(merged);
    }

    clusters = real_clusters;

    // Log cluster distribution
    for (ci, cluster) in clusters.iter().enumerate() {
        eprintln!(
            "[diarization] cluster {}: {} segments ({:.1}%)",
            ci, cluster.len(), cluster.len() as f64 / n as f64 * 100.0
        );
    }

    // Chronological speaker ID assignment (first to speak = "Speaker 1")
    let mut segment_labels = vec![0; n];
    for (cluster_id, cluster) in clusters.iter().enumerate() {
        for &idx in cluster {
            segment_labels[idx] = cluster_id;
        }
    }

    let mut appearance_order = Vec::new();
    for &lbl in &segment_labels {
        if !appearance_order.contains(&lbl) {
            appearance_order.push(lbl);
        }
    }

    let mut result: Vec<SpeakerSegment> = Vec::new();
    for (idx, segment) in valid_segments.into_iter().enumerate() {
        let speaker_idx = appearance_order
            .iter()
            .position(|&x| x == segment_labels[idx])
            .unwrap();
        result.push(SpeakerSegment {
            start: segment.start,
            end: segment.end,
            speaker: format!("Speaker {}", speaker_idx + 1),
        });
    }

    result.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let merged = merge_consecutive_segments(&result, merge_gap);

    eprintln!(
        "[diarization] complete: {} clusters, {} merged segments",
        appearance_order.len(),
        merged.len()
    );
    info!("Diarization complete: {} segments", merged.len());
    Ok(merged)
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }
    (1.0 - (dot / (norm_a.sqrt() * norm_b.sqrt()))).max(0.0)
}

/// Merge consecutive segments that have the same speaker label.
fn merge_consecutive_segments(segments: &[SpeakerSegment], merge_gap: f64) -> Vec<SpeakerSegment> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut merged: Vec<SpeakerSegment> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.speaker == seg.speaker && (seg.start - last.end).abs() <= merge_gap {
                last.end = seg.end.max(last.end);
                continue;
            }
        }
        merged.push(seg.clone());
    }
    merged
}

/// Convert f32 mono 16kHz audio to i16 samples (for pyannote-rs).
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

/// Word-level diarized text formatting.
/// Each text segment has precise (start, end) timestamps.
/// Assigns each segment to the speaker who was active at its midpoint.
/// Groups consecutive same-speaker words into contiguous blocks.
pub fn format_diarized_text(
    text_segments: &[(f64, f64, String)],
    speaker_segments: &[SpeakerSegment],
) -> String {
    if speaker_segments.is_empty() || text_segments.is_empty() {
        return text_segments
            .iter()
            .map(|(_, _, t)| t.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_speaker: Option<String> = None;
    let mut current_words: Vec<String> = Vec::new();

    for (start, end, text) in text_segments {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Find speaker at the midpoint of this word/segment
        let mid = (*start + *end) / 2.0;
        let speaker = find_speaker_at_time(mid, speaker_segments);

        if current_speaker.as_deref() != Some(speaker.as_str()) {
            // Flush accumulated words for the previous speaker
            if !current_words.is_empty() {
                lines.push(current_words.join(" "));
                current_words.clear();
            }
            current_speaker = Some(speaker.clone());
            lines.push(format!("\n[{}|{:.1}]", speaker, start));
        }

        current_words.push(trimmed.to_string());
    }

    // Flush remaining words
    if !current_words.is_empty() {
        lines.push(current_words.join(" "));
    }

    lines.join("\n").trim().to_string()
}

/// Find which speaker is active at a given time point.
/// Returns the speaker of the segment containing the time, or the closest segment.
fn find_speaker_at_time(time: f64, segments: &[SpeakerSegment]) -> String {
    // First: check if time falls within any segment
    for seg in segments {
        if time >= seg.start && time <= seg.end {
            return seg.speaker.clone();
        }
    }

    // Fallback: find the closest segment
    let mut closest = "Speaker ?".to_string();
    let mut min_dist = f64::MAX;
    for seg in segments {
        let dist = if time < seg.start {
            seg.start - time
        } else {
            time - seg.end
        };
        if dist < min_dist {
            min_dist = dist;
            closest = seg.speaker.clone();
        }
    }
    closest
}
```

## Файл 2: `src-tauri/src/commands/transcription.rs` (вызывающий код)

```rust
// Commands for transcription: start, get result, open result window.

use crate::commands::models::SelectedModelState;
use crate::managers::transcription::{
    load_transcription_chat_history, load_transcription_metadata, load_transcription_result,
    save_transcription_chat_history, save_transcription_metadata, save_transcription_result,
    ChatHistoryMessage, TranscriptionManager, TranscriptionState,
};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use futures_util::StreamExt;
use hound::WavReader;
use rubato::{FftFixedIn, Resampler};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use std::time::Instant;

#[derive(Clone, Serialize)]
pub struct TranscriptionStatusEvent {
    pub recording_path: String,
    pub status: String, // "started" | "completed" | "error"
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionPhaseEvent {
    pub recording_path: String,
    pub phase: String, // "loading-model" | "preparing-audio" | "transcribing"
}

#[derive(Clone, Serialize)]
pub struct TranscriptionProgressEvent {
    pub recording_path: String,
    pub progress: f32, // 0.0 - 1.0
    pub eta_seconds: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionOpenEvent {
    pub recording_path: String,
}

#[tauri::command]
pub async fn start_transcription(
    app: AppHandle,
    recording_path: String,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    selected_model_state: State<'_, SelectedModelState>,
) -> Result<(), String> {
    transcription_manager.inner().set_state(
        &recording_path,
        TranscriptionState {
            status: "started".to_string(),
            progress: 0.0,
            eta_seconds: None,
            phase: Some("preparing-audio".to_string()),
        },
    );
    let _ = app.emit(
        "transcription-status",
        TranscriptionStatusEvent {
            recording_path: recording_path.clone(),
            status: "started".to_string(),
            error: None,
        },
    );

    let app_clone = app.clone();
    let path_clone = recording_path.clone();
    let tm = Arc::clone(transcription_manager.inner());
    let sel = selected_model_state.0.clone();
    let cancel_flag = tm.create_cancel_flag(&recording_path);

    std::thread::spawn(move || {
        let result = run_transcription(&app_clone, &path_clone, &tm, &sel, &cancel_flag);
        tm.remove_cancel_flag(&path_clone);
        let (status, err) = match result {
            Ok(()) => {
                if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    ("cancelled".to_string(), None)
                } else {
                    ("completed".to_string(), None)
                }
            }
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };
        tm.set_state(
            &path_clone,
            TranscriptionState {
                status: status.clone(),
                progress: if status == "completed" { 1.0 } else { 0.0 },
                eta_seconds: if status == "completed" { Some(0) } else { None },
                phase: None,
            },
        );
        let _ = app_clone.emit(
            "transcription-status",
            TranscriptionStatusEvent {
                recording_path: path_clone,
                status,
                error: err,
            },
        );
    });

    Ok(())
}

fn run_transcription(
    app: &AppHandle,
    recording_path: &str,
    tm: &TranscriptionManager,
    selected_model: &Arc<std::sync::Mutex<String>>,
    cancel_flag: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    eprintln!("[transcription] run_transcription called for: {}", recording_path);
    let model_id = {
        let sel = selected_model.lock().map_err(|e| e.to_string())?;
        sel.clone()
    };
    if model_id.is_empty() || model_id == "none" {
        return Err("No transcription model selected. Choose a model in the bottom left corner.".into());
    }

    // Load diarization settings
    let app_settings = crate::llm_settings::load_app_settings(app).unwrap_or_default();
    let diarization_enabled = app_settings.diarization_enabled == "true";
    let diarization_max_speakers: usize = app_settings.diarization_max_speakers.parse().unwrap_or(3);
    let diarization_threshold: f64 = app_settings.diarization_threshold.parse().unwrap_or(0.50);
    let diarization_merge_gap: f64 = app_settings.diarization_merge_gap.parse().unwrap_or(2.5);
    eprintln!(
        "[transcription] diarization: enabled={}, max_speakers={}, threshold={}, merge_gap={}",
        diarization_enabled, diarization_max_speakers, diarization_threshold, diarization_merge_gap
    );

    let _ = app.emit(
        "transcription-phase",
        TranscriptionPhaseEvent {
            recording_path: recording_path.to_string(),
            phase: "preparing-audio".to_string(),
        },
    );
    tm.set_state(
        recording_path,
        TranscriptionState {
            status: "started".to_string(),
            progress: 0.0,
            eta_seconds: None,
            phase: Some("preparing-audio".to_string()),
        },
    );

    let current = tm.get_current_model();
    if current.as_deref() != Some(model_id.as_str()) {
        let _ = app.emit(
            "transcription-phase",
            TranscriptionPhaseEvent {
                recording_path: recording_path.to_string(),
                phase: "loading-model".to_string(),
            },
        );
        tm.set_state(
            recording_path,
            TranscriptionState {
                status: "started".to_string(),
                progress: 0.0,
                eta_seconds: None,
                phase: Some("loading-model".to_string()),
            },
        );
        tm.load_model(&model_id)?;
    }

    const TARGET_SAMPLE_RATE: usize = 16000;
    const RESAMPLER_CHUNK: usize = 1024;
    const TRANSCRIBE_CHUNK_SECONDS: usize = 30;
    let transcribe_chunk_samples = TRANSCRIBE_CHUNK_SECONDS * TARGET_SAMPLE_RATE;

    let mut reader = WavReader::open(Path::new(recording_path))?;
    let spec = reader.spec();
    let sample_rate_in = spec.sample_rate as usize;
    let channels = spec.channels as usize;

    let total_input_samples = reader.len() as usize;
    let total_frames_in = if channels > 0 {
        total_input_samples / channels
    } else {
        0
    };
    if total_frames_in == 0 {
        save_transcription_result(app, recording_path, "")?;
        save_transcription_metadata(app, recording_path, &model_id)?;
        return Ok(());
    }

    let total_seconds = total_frames_in as f32 / sample_rate_in as f32;
    let total_out_samples = (total_seconds * TARGET_SAMPLE_RATE as f32).round() as usize;

    let mut resampler = if sample_rate_in == TARGET_SAMPLE_RATE {
        None
    } else {
        Some(FftFixedIn::<f32>::new(
            sample_rate_in,
            TARGET_SAMPLE_RATE,
            RESAMPLER_CHUNK,
            1,
            1,
        )?)
    };

    let mut input_mono: Vec<f32> = Vec::with_capacity(RESAMPLER_CHUNK);
    let mut pending_16k: Vec<f32> = Vec::with_capacity(transcribe_chunk_samples);
    let mut processed_out_samples = 0usize;
    let start = Instant::now();
    // Store (start_time_seconds, end_time_seconds, text) for each segment -- needed for diarization alignment
    let mut parts: Vec<(f64, f64, String)> = Vec::new();
    let mut transcription_started = false;

    // Collect all resampled audio for diarization (only if enabled)
    let mut all_audio_16k: Vec<f32> = if diarization_enabled {
        Vec::with_capacity(total_out_samples)
    } else {
        Vec::new()
    };

    let emit_progress = |app: &AppHandle,
                         tm: &TranscriptionManager,
                         recording_path: &str,
                         progress: f32,
                         eta_seconds: Option<u64>| {
        tm.set_state(
            recording_path,
            TranscriptionState {
                status: "transcribing".to_string(),
                progress,
                eta_seconds,
                phase: Some("transcribing".to_string()),
            },
        );
        let _ = app.emit(
            "transcription-progress",
            TranscriptionProgressEvent {
                recording_path: recording_path.to_string(),
                progress,
                eta_seconds,
            },
        );
    };

    let mut process_pending = |pending_16k: &mut Vec<f32>| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        while pending_16k.len() >= transcribe_chunk_samples {
            if cancel_flag.load(Ordering::Relaxed) {
                return Ok(());
            }
            let chunk: Vec<f32> = pending_16k.drain(..transcribe_chunk_samples).collect();
            if diarization_enabled {
                all_audio_16k.extend_from_slice(&chunk);
            }
            let chunk_start_seconds = processed_out_samples as f64 / TARGET_SAMPLE_RATE as f64;
            if !transcription_started {
                transcription_started = true;
                let _ = app.emit(
                    "transcription-phase",
                    TranscriptionPhaseEvent {
                        recording_path: recording_path.to_string(),
                        phase: "transcribing".to_string(),
                    },
                );
            }
            if diarization_enabled {
                // Word-level timestamps for precise speaker alignment
                let word_segments = tm.transcribe_with_timestamps(chunk, chunk_start_seconds)?;
                for (start, end, text) in word_segments {
                    if !text.trim().is_empty() {
                        parts.push((start, end, text));
                    }
                }
            } else {
                let chunk_text = tm.transcribe(chunk)?;
                if !chunk_text.trim().is_empty() {
                    let chunk_end_seconds = (processed_out_samples + transcribe_chunk_samples) as f64 / TARGET_SAMPLE_RATE as f64;
                    parts.push((chunk_start_seconds, chunk_end_seconds, chunk_text));
                }
            }
            processed_out_samples = processed_out_samples.saturating_add(transcribe_chunk_samples);
            let progress = if total_out_samples > 0 {
                (processed_out_samples as f32 / total_out_samples as f32).min(1.0)
            } else {
                0.0
            };
            let processed_seconds = processed_out_samples as f32 / TARGET_SAMPLE_RATE as f32;
            let eta_seconds = if processed_seconds > 0.5 {
                let elapsed = start.elapsed().as_secs_f32();
                let rate = elapsed / processed_seconds;
                let remaining_seconds = (total_seconds - processed_seconds).max(0.0) * rate;
                Some(remaining_seconds.round() as u64)
            } else {
                None
            };
            emit_progress(app, tm, recording_path, progress, eta_seconds);
        }
        Ok(())
    };

    let mut channel_index = 0usize;
    match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = 32768.0f32;
            for s in reader.samples::<i16>() {
                let s = s?;
                if channel_index == 0 {
                    input_mono.push(s as f32 / max_val);
                }
                channel_index = (channel_index + 1) % channels.max(1);
                if input_mono.len() >= RESAMPLER_CHUNK {
                    if let Some(resampler) = resampler.as_mut() {
                        let out_chunk = resampler.process(&[&input_mono[..RESAMPLER_CHUNK]], None)?;
                        pending_16k.extend_from_slice(&out_chunk[0]);
                    } else {
                        pending_16k.extend_from_slice(&input_mono[..RESAMPLER_CHUNK]);
                    }
                    input_mono.clear();
                    process_pending(&mut pending_16k)?;
                }
            }
        }
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                let s = s?;
                if channel_index == 0 {
                    input_mono.push(s);
                }
                channel_index = (channel_index + 1) % channels.max(1);
                if input_mono.len() >= RESAMPLER_CHUNK {
                    if let Some(resampler) = resampler.as_mut() {
                        let out_chunk = resampler.process(&[&input_mono[..RESAMPLER_CHUNK]], None)?;
                        pending_16k.extend_from_slice(&out_chunk[0]);
                    } else {
                        pending_16k.extend_from_slice(&input_mono[..RESAMPLER_CHUNK]);
                    }
                    input_mono.clear();
                    process_pending(&mut pending_16k)?;
                }
            }
        }
    }

    if !input_mono.is_empty() {
        if let Some(resampler) = resampler.as_mut() {
            let mut pad = input_mono;
            pad.resize(RESAMPLER_CHUNK, 0.0);
            let out_chunk = resampler.process(&[&pad], None)?;
            pending_16k.extend_from_slice(&out_chunk[0]);
        } else {
            pending_16k.extend_from_slice(&input_mono);
        }
        process_pending(&mut pending_16k)?;
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(());
    }

    if !pending_16k.is_empty() {
        if !transcription_started {
            let _ = app.emit(
                "transcription-phase",
                TranscriptionPhaseEvent {
                    recording_path: recording_path.to_string(),
                    phase: "transcribing".to_string(),
                },
            );
        }
        let chunk: Vec<f32> = pending_16k.drain(..).collect();
        if diarization_enabled {
            all_audio_16k.extend_from_slice(&chunk);
        }
        let chunk_start_seconds = processed_out_samples as f64 / TARGET_SAMPLE_RATE as f64;
        let chunk_len = chunk.len();
        if diarization_enabled {
            let word_segments = tm.transcribe_with_timestamps(chunk, chunk_start_seconds)?;
            for (start, end, text) in word_segments {
                if !text.trim().is_empty() {
                    parts.push((start, end, text));
                }
            }
        } else {
            let chunk_text = tm.transcribe(chunk)?;
            if !chunk_text.trim().is_empty() {
                let chunk_end_seconds = (processed_out_samples + chunk_len) as f64 / TARGET_SAMPLE_RATE as f64;
                parts.push((chunk_start_seconds, chunk_end_seconds, chunk_text));
            }
        }
        processed_out_samples = processed_out_samples.saturating_add(chunk_len);
        let progress = if total_out_samples > 0 {
            (processed_out_samples as f32 / total_out_samples as f32).min(1.0)
        } else {
            1.0
        };
        emit_progress(app, tm, recording_path, progress, Some(0));
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(());
    }

    // Run diarization if enabled
    let text = if diarization_enabled && !all_audio_16k.is_empty() {
        let _ = app.emit(
            "transcription-phase",
            TranscriptionPhaseEvent {
                recording_path: recording_path.to_string(),
                phase: "diarizing".to_string(),
            },
        );
        tm.set_state(
            recording_path,
            TranscriptionState {
                status: "started".to_string(),
                progress: 0.95,
                eta_seconds: None,
                phase: Some("diarizing".to_string()),
            },
        );

        // Get diarization model paths
        let model_manager: &Arc<crate::managers::model::ModelManager> = &*app.state();
        let seg_path = model_manager.get_model_path("diarize-segmentation");
        let emb_path = model_manager.get_model_path("diarize-embedding");

        eprintln!("[transcription] diarization model paths: seg={:?}, emb={:?}", seg_path, emb_path);
        match (seg_path, emb_path) {
            (Ok(seg), Ok(emb)) => {
                // Try reading audio directly via pyannote's own reader for comparison
                // Convert resampled f32 audio to i16 for diarization
                // IMPORTANT: Use all_audio_16k (16kHz mono) NOT raw WAV samples
                eprintln!("[transcription] converting {} samples @ 16kHz to i16", all_audio_16k.len());
                let samples_i16 = crate::managers::diarization::f32_to_i16(&all_audio_16k);
                let sr = TARGET_SAMPLE_RATE as u32; // 16000 Hz

                match crate::managers::diarization::run_diarization(
                    &samples_i16,
                    sr,
                    &seg,
                    &emb,
                    diarization_max_speakers,
                    diarization_threshold,
                    diarization_merge_gap,
                ) {
                    Ok(speaker_segments) => {
                        eprintln!("[transcription] diarization OK: {} speaker segments found", speaker_segments.len());
                        let formatted = crate::managers::diarization::format_diarized_text(&parts, &speaker_segments);
                        eprintln!("[transcription] diarized text length: {} chars", formatted.len());
                        formatted
                    }
                    Err(e) => {
                        eprintln!("[transcription] diarization FAILED: {}", e);
                        parts.iter().map(|(_, _, t)| t.as_str()).collect::<Vec<_>>().join(" ")
                    }
                }
            }
            _ => {
                eprintln!("[transcription] diarization models not downloaded, falling back to plain text");
                parts.iter().map(|(_, _, t)| t.as_str()).collect::<Vec<_>>().join(" ")
            }
        }
    } else {
        parts.iter().map(|(_, _, t)| t.as_str()).collect::<Vec<_>>().join(" ")
    };

    save_transcription_result(app, recording_path, &text)?;
    save_transcription_metadata(app, recording_path, &model_id)?;
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_result(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_transcription_model(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_metadata(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_transcription_state(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    recording_path: String,
) -> Result<Option<TranscriptionState>, String> {
    Ok(transcription_manager.inner().get_state(&recording_path))
}

#[tauri::command]
pub async fn open_transcription_window(app: AppHandle, recording_path: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("transcription-result") {
        let _ = window.emit(
            "transcription-open",
            TranscriptionOpenEvent {
                recording_path: recording_path.clone(),
            },
        );
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let encoded = urlencoding::encode(&recording_path);
    let url = WebviewUrl::App(format!("index.html?recording_path={}", encoded).into());
    WebviewWindowBuilder::new(&app, "transcription-result", url)
        .title("Transcription")
        .inner_size(620.0, 700.0)
        .min_inner_size(400.0, 300.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn has_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<bool, String> {
    let path = crate::managers::transcription::transcription_result_path(&app, &recording_path)
        .map_err(|e| e.to_string())?;
    Ok(path.exists())
}

#[tauri::command]
pub async fn cancel_transcription(
    app: AppHandle,
    recording_path: String,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<(), String> {
    let found = transcription_manager.inner().cancel(&recording_path);
    if found {
        transcription_manager.inner().set_state(
            &recording_path,
            TranscriptionState {
                status: "cancelled".to_string(),
                progress: 0.0,
                eta_seconds: None,
                phase: None,
            },
        );
        let _ = app.emit(
            "transcription-status",
            TranscriptionStatusEvent {
                recording_path,
                status: "cancelled".to_string(),
                error: None,
            },
        );
    }
    Ok(())
}

#[tauri::command]
pub async fn get_all_transcription_states(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<std::collections::HashMap<String, TranscriptionState>, String> {
    Ok(transcription_manager.inner().get_all_states())
}

/// Get LLM settings (endpoint and model, omit API key for security)
#[tauri::command]
pub async fn get_llm_settings(app: AppHandle) -> Result<crate::llm_settings::LlmSettingsPublic, String> {
    let settings = crate::llm_settings::load_llm_settings(&app).map_err(|e| e.to_string())?;
    Ok(crate::llm_settings::LlmSettingsPublic {
        endpoint: settings.endpoint,
        model: settings.model,
    })
}

/// Set LLM settings (endpoint, API key, model)
#[tauri::command]
pub async fn set_llm_settings(
    app: AppHandle,
    endpoint: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let settings = crate::llm_settings::LlmSettings {
        endpoint,
        api_key,
        model,
    };
    crate::llm_settings::save_llm_settings(&app, &settings).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessageDto {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionChatStreamEvent {
    pub chat_id: String,
    pub delta: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionChatDoneEvent {
    pub chat_id: String,
}

/// Stream LLM chat responses based on transcription + conversation history
#[tauri::command]
pub async fn stream_transcription_chat(
    app: AppHandle,
    recording_path: String,
    messages: Vec<ChatMessageDto>,
    chat_id: String,
) -> Result<(), String> {
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = do_stream_chat(&app_clone, &recording_path, messages, &chat_id).await {
            let _ = app_clone.emit(
                "transcription-chat-error",
                TranscriptionChatStreamEvent {
                    chat_id,
                    delta: format!("Error: {}", e),
                },
            );
        }
    });
    Ok(())
}

/// Load saved chat history for a transcription.
#[tauri::command]
pub async fn get_transcription_chat_history(
    app: AppHandle,
    recording_path: String,
) -> Result<Vec<ChatMessageDto>, String> {
    let messages = load_transcription_chat_history(&app, &recording_path).map_err(|e| e.to_string())?;
    Ok(messages
        .into_iter()
        .map(|m| ChatMessageDto {
            role: m.role,
            content: m.content,
        })
        .collect())
}

/// Save chat history for a transcription.
#[tauri::command]
pub async fn set_transcription_chat_history(
    app: AppHandle,
    recording_path: String,
    messages: Vec<ChatMessageDto>,
) -> Result<(), String> {
    let normalized: Vec<ChatHistoryMessage> = messages
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatHistoryMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    save_transcription_chat_history(&app, &recording_path, &normalized)
        .map_err(|e| e.to_string())
}

async fn do_stream_chat(
    app: &AppHandle,
    recording_path: &str,
    messages: Vec<ChatMessageDto>,
    chat_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = crate::llm_settings::load_llm_settings(app)?;
    if settings.api_key.is_empty() {
        return Err("API key not configured. Set it in Settings.".into());
    }

    let transcription = load_transcription_result(app, recording_path)?
        .unwrap_or_else(|| "(No transcription)".to_string());

    let config = OpenAIConfig::new()
        .with_api_key(&settings.api_key)
        .with_api_base(&settings.endpoint);
    let client = Client::with_config(config);

    let mut openai_messages = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(format!(
                "You are a helpful assistant. The user has a transcription:\n\n{}\n\nAnswer questions about it.",
                transcription
            ))
            .build()?
            .into(),
    ];

    for msg in messages {
        let role: ChatCompletionRequestMessage = match msg.role.as_str() {
            "user" => ChatCompletionRequestUserMessageArgs::default()
                .content(msg.content)
                .build()?
                .into(),
            "assistant" => ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.content)
                .build()?
                .into(),
            _ => continue,
        };
        openai_messages.push(role);
    }

    let request = CreateChatCompletionRequestArgs::default()
        .model(&settings.model)
        .messages(openai_messages)
        .build()?;

    let mut stream = client.chat().create_stream(request).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                for choice in response.choices {
                    if let Some(ref content) = choice.delta.content {
                        let _ = app.emit(
                            "transcription-chat-stream",
                            TranscriptionChatStreamEvent {
                                chat_id: chat_id.to_string(),
                                delta: content.clone(),
                            },
                        );
                    }
                }
            }
            Err(e) => {
                return Err(format!("Stream error: {}", e).into());
            }
        }
    }

    let _ = app.emit(
        "transcription-chat-done",
        TranscriptionChatDoneEvent {
            chat_id: chat_id.to_string(),
        },
    );

    Ok(())
}
```

## Файл 3: `src-tauri/src/managers/transcription.rs` (TranscriptionManager)

```rust
// Transcription: load model, run inference on file. Adapted from Handy (open license).

use crate::managers::model::{EngineType, ModelManager};
use anyhow::Result;
use log::{debug, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use transcribe_rs::{
    engines::{
        moonshine::{ModelVariant, MoonshineEngine, MoonshineModelParams},
        parakeet::{
            ParakeetEngine, ParakeetInferenceParams, ParakeetModelParams, TimestampGranularity,
        },
        whisper::{WhisperEngine, WhisperInferenceParams},
    },
    TranscriptionEngine,
};

enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetEngine),
    Moonshine(MoonshineEngine),
}

pub struct TranscriptionManager {
    engine: Mutex<Option<LoadedEngine>>,
    current_model_id: Mutex<Option<String>>,
    state: Mutex<HashMap<String, TranscriptionState>>,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
    model_manager: Arc<ModelManager>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionState {
    pub status: String,
    pub progress: f32,
    pub eta_seconds: Option<u64>,
    pub phase: Option<String>,
}

impl TranscriptionManager {
    pub fn new(model_manager: Arc<ModelManager>) -> Self {
        Self {
            engine: Mutex::new(None),
            current_model_id: Mutex::new(None),
            state: Mutex::new(HashMap::new()),
            cancel_flags: Mutex::new(HashMap::new()),
            model_manager,
        }
    }

    pub fn get_current_model(&self) -> Option<String> {
        self.current_model_id.lock().unwrap().clone()
    }

    pub fn set_state(&self, recording_path: &str, state: TranscriptionState) {
        self.state
            .lock()
            .unwrap()
            .insert(recording_path.to_string(), state);
    }

    pub fn get_state(&self, recording_path: &str) -> Option<TranscriptionState> {
        self.state.lock().unwrap().get(recording_path).cloned()
    }

    pub fn create_cancel_flag(&self, recording_path: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.cancel_flags
            .lock()
            .unwrap()
            .insert(recording_path.to_string(), flag.clone());
        flag
    }

    pub fn cancel(&self, recording_path: &str) -> bool {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(recording_path) {
            flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn remove_cancel_flag(&self, recording_path: &str) {
        self.cancel_flags.lock().unwrap().remove(recording_path);
    }

    pub fn get_all_states(&self) -> HashMap<String, TranscriptionState> {
        self.state.lock().unwrap().clone()
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not downloaded"));
        }
        let model_path = self.model_manager.get_model_path(model_id)?;

        let loaded = match model_info.engine_type {
            EngineType::Whisper => {
                let mut engine = WhisperEngine::new();
                engine.load_model(&model_path)
                    .map_err(|e| anyhow::anyhow!("Whisper load failed: {}", e))?;
                LoadedEngine::Whisper(engine)
            }
            EngineType::Parakeet => {
                let mut engine = ParakeetEngine::new();
                engine
                    .load_model_with_params(&model_path, ParakeetModelParams::int8())
                    .map_err(|e| anyhow::anyhow!("Parakeet load failed: {}", e))?;
                LoadedEngine::Parakeet(engine)
            }
            EngineType::Moonshine => {
                let mut engine = MoonshineEngine::new();
                engine
                    .load_model_with_params(
                        &model_path,
                        MoonshineModelParams::variant(ModelVariant::Base),
                    )
                    .map_err(|e| anyhow::anyhow!("Moonshine load failed: {}", e))?;
                LoadedEngine::Moonshine(engine)
            }
        };

        *self.engine.lock().unwrap() = Some(loaded);
        *self.current_model_id.lock().unwrap() = Some(model_id.to_string());
        debug!("Transcription model loaded: {}", model_id);
        Ok(())
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }
        let mut engine_guard = self.engine.lock().unwrap();
        let engine = engine_guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Model not loaded. Select and load a model first.")
        })?;

        let result = match engine {
            LoadedEngine::Whisper(e) => e
                .transcribe_samples(audio, Some(WhisperInferenceParams::default()))
                .map_err(|x| anyhow::anyhow!("Whisper: {}", x))?,
            LoadedEngine::Parakeet(e) => e
                .transcribe_samples(
                    audio,
                    Some(ParakeetInferenceParams {
                        timestamp_granularity: TimestampGranularity::Segment,
                        ..Default::default()
                    }),
                )
                .map_err(|x| anyhow::anyhow!("Parakeet: {}", x))?,
            LoadedEngine::Moonshine(e) => e
                .transcribe_samples(audio, None)
                .map_err(|x| anyhow::anyhow!("Moonshine: {}", x))?,
        };

        let text = result.text.trim().to_string();
        if text.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription length: {} chars", text.len());
        }
        Ok(text)
    }

    /// Transcribe audio and return word-level segments with timestamps.
    /// Returns Vec<(start_seconds, end_seconds, word_text)>.
    /// For Parakeet: uses Word-level timestamps for precise per-word timing.
    /// For Whisper/Moonshine: returns single segment per chunk (fallback).
    pub fn transcribe_with_timestamps(
        &self,
        audio: Vec<f32>,
        chunk_offset_seconds: f64,
    ) -> Result<Vec<(f64, f64, String)>> {
        if audio.is_empty() {
            return Ok(Vec::new());
        }
        let mut engine_guard = self.engine.lock().unwrap();
        let engine = engine_guard.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Model not loaded. Select and load a model first.")
        })?;

        let result = match engine {
            LoadedEngine::Parakeet(e) => e
                .transcribe_samples(
                    audio.clone(),
                    Some(ParakeetInferenceParams {
                        timestamp_granularity: TimestampGranularity::Word,
                        ..Default::default()
                    }),
                )
                .map_err(|x| anyhow::anyhow!("Parakeet: {}", x))?,
            LoadedEngine::Whisper(e) => e
                .transcribe_samples(audio.clone(), Some(WhisperInferenceParams::default()))
                .map_err(|x| anyhow::anyhow!("Whisper: {}", x))?,
            LoadedEngine::Moonshine(e) => e
                .transcribe_samples(audio.clone(), None)
                .map_err(|x| anyhow::anyhow!("Moonshine: {}", x))?,
        };

        let text = result.text.trim().to_string();
        if text.is_empty() {
            return Ok(Vec::new());
        }

        // If we have segments (word timestamps), use them
        if let Some(segments) = result.segments {
            if !segments.is_empty() {
                let word_segments: Vec<(f64, f64, String)> = segments
                    .into_iter()
                    .filter(|s| !s.text.trim().is_empty())
                    .map(|s| {
                        (
                            chunk_offset_seconds + s.start as f64,
                            chunk_offset_seconds + s.end as f64,
                            s.text,
                        )
                    })
                    .collect();
                info!("Transcription with {} word segments", word_segments.len());
                return Ok(word_segments);
            }
        }

        // Fallback: return whole text as single segment
        let chunk_duration = audio.len() as f64 / 16000.0;
        info!("Transcription fallback: single segment, {} chars", text.len());
        Ok(vec![(
            chunk_offset_seconds,
            chunk_offset_seconds + chunk_duration,
            text,
        )])
    }
}

/// Base directory for transcriptions: ~/Documents/Crispy/Transcriptions (next to Recordings and settings).
fn transcriptions_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = crate::paths::transcriptions_dir(app)
        .map_err(|e| anyhow::anyhow!(e))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Store transcription result by recording path. Uses a hash of path as filename.
pub fn transcription_result_path(_app: &AppHandle, recording_path: &str) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.txt", name)))
}

fn transcription_file_stem(recording_path: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    recording_path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Path to metadata file (model_id) for a transcription. Same stem as .txt but .meta.
pub fn transcription_metadata_path(_app: &AppHandle, recording_path: &str) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.meta", name)))
}

/// Path to chat history file for a transcription. Same stem as .txt but .chat.json.
pub fn transcription_chat_history_path(
    _app: &AppHandle,
    recording_path: &str,
) -> Result<PathBuf> {
    let dir = transcriptions_dir(_app)?;
    let name = transcription_file_stem(recording_path);
    Ok(dir.join(format!("{}.chat.json", name)))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TranscriptionMetadata {
    model_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ChatHistoryMessage {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

pub fn save_transcription_result(app: &AppHandle, recording_path: &str, text: &str) -> Result<()> {
    let path = transcription_result_path(app, recording_path)?;
    std::fs::write(&path, text)?;
    Ok(())
}

pub fn save_transcription_metadata(app: &AppHandle, recording_path: &str, model_id: &str) -> Result<()> {
    let path = transcription_metadata_path(app, recording_path)?;
    let meta = TranscriptionMetadata {
        model_id: model_id.to_string(),
    };
    let json = serde_json::to_string(&meta)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load_transcription_result(app: &AppHandle, recording_path: &str) -> Result<Option<String>> {
    let path = transcription_result_path(app, recording_path)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(Some(text))
}

pub fn load_transcription_metadata(app: &AppHandle, recording_path: &str) -> Result<Option<String>> {
    let path = transcription_metadata_path(app, recording_path)?;
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)?;
    let meta: TranscriptionMetadata = serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("metadata: {}", e))?;
    Ok(Some(meta.model_id))
}

pub fn save_transcription_chat_history(
    app: &AppHandle,
    recording_path: &str,
    messages: &[ChatHistoryMessage],
) -> Result<()> {
    let path = transcription_chat_history_path(app, recording_path)?;
    let json = serde_json::to_string_pretty(messages)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load_transcription_chat_history(
    app: &AppHandle,
    recording_path: &str,
) -> Result<Vec<ChatHistoryMessage>> {
    let path = transcription_chat_history_path(app, recording_path)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let json = std::fs::read_to_string(&path)?;
    let messages: Vec<ChatHistoryMessage> =
        serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("chat history: {}", e))?;
    Ok(messages)
}
```

## Файл 4: `src-tauri/Cargo.toml` (зависимости, только релевантные строки)

```toml
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
transcribe-rs = { version = "0.2.2", features = ["whisper", "parakeet", "moonshine"] }
ort = "=2.0.0-rc.10"
ort-sys = "=2.0.0-rc.10"
# Noise suppression: RNNoise port (path to ./rnnnoise in project root)
pyannote-rs = "0.3"
ndarray = "0.16"
panic = "abort"
```

## Файл 5: `src/components/settings/DiarizationToggle.tsx` (UI настройки)

```tsx
import React, { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useTauriListen } from "../../hooks/useTauriListen";
import { Download, Loader2, CheckCircle2, ChevronDown, ChevronUp, RotateCcw } from "lucide-react";

interface ModelInfo {
  id: string;
  name: string;
  is_downloaded: boolean;
  is_downloading: boolean;
  size_mb: number;
}

interface DownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

interface DiarizationToggleProps {
  grouped?: boolean;
}

const DIARIZATION_MODELS = ["diarize-segmentation", "diarize-embedding"];

const DEFAULTS = {
  max_speakers: "3",
  threshold: "0.50",
  merge_gap: "2.5",
};

export const DiarizationToggle: React.FC<DiarizationToggleProps> = ({
  grouped = false,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const enabled = getSetting("diarization_enabled") === "true";

  const [modelsReady, setModelsReady] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [modelStatuses, setModelStatuses] = useState<Record<string, boolean>>({});
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Diarization hyperparameters
  const maxSpeakers = Number.parseInt(getSetting("diarization_max_speakers") || DEFAULTS.max_speakers, 10);
  const threshold = Number.parseFloat(getSetting("diarization_threshold") || DEFAULTS.threshold);
  const mergeGap = Number.parseFloat(getSetting("diarization_merge_gap") || DEFAULTS.merge_gap);

  const checkModels = useCallback(async () => {
    try {
      const statuses: Record<string, boolean> = {};
      let allReady = true;
      for (const modelId of DIARIZATION_MODELS) {
        const info = await invoke<ModelInfo | null>("get_model_info", { modelId });
        statuses[modelId] = info?.is_downloaded ?? false;
        if (!info?.is_downloaded) allReady = false;
      }
      setModelStatuses(statuses);
      setModelsReady(allReady);
    } catch {
      setModelsReady(false);
    }
  }, []);

  useEffect(() => {
    checkModels();
  }, [checkModels]);

  useTauriListen<DownloadProgress>("model-download-progress", (event) => {
    if (DIARIZATION_MODELS.includes(event.payload.model_id)) {
      setDownloadProgress((prev) => ({
        ...prev,
        [event.payload.model_id]: event.payload.percentage,
      }));
    }
  });

  useTauriListen<string>("model-download-complete", (event) => {
    if (DIARIZATION_MODELS.includes(event.payload)) {
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[event.payload];
        return next;
      });
      checkModels();
    }
  });

  const handleToggle = async () => {
    if (!enabled && !modelsReady) {
      await downloadModels();
      return;
    }
    updateSetting("diarization_enabled", enabled ? "false" : "true");
  };

  const downloadModels = async () => {
    setDownloading(true);
    try {
      for (const modelId of DIARIZATION_MODELS) {
        if (!modelStatuses[modelId]) {
          await invoke("download_model", { modelId });
        }
      }
      await checkModels();
      updateSetting("diarization_enabled", "true");
    } catch (err) {
      console.error("Failed to download diarization models:", err);
    } finally {
      setDownloading(false);
    }
  };

  const resetToDefaults = () => {
    updateSetting("diarization_max_speakers", DEFAULTS.max_speakers);
    updateSetting("diarization_threshold", DEFAULTS.threshold);
    updateSetting("diarization_merge_gap", DEFAULTS.merge_gap);
  };

  const isDefault =
    maxSpeakers === Number.parseInt(DEFAULTS.max_speakers, 10) &&
    threshold === Number.parseFloat(DEFAULTS.threshold) &&
    mergeGap === Number.parseFloat(DEFAULTS.merge_gap);

  const totalProgress =
    Object.values(downloadProgress).length > 0
      ? Object.values(downloadProgress).reduce((a, b) => a + b, 0) /
        DIARIZATION_MODELS.length
      : 0;

  return (
    <div>
      <SettingContainer
        title="Speaker Diarization"
        description={
          modelsReady
            ? "Identify different speakers in transcriptions."
            : "Identify different speakers in transcriptions. Requires downloading diarization models (~34 MB)."
        }
        grouped={grouped}
        layout="horizontal"
      >
        <div className="flex items-center gap-3">
          {downloading && (
            <div className="flex items-center gap-2 text-xs text-mid-gray">
              <Loader2 size={14} className="animate-spin" />
              <span>{Math.round(totalProgress)}%</span>
            </div>
          )}

          {!modelsReady && !downloading && (
            <button
              type="button"
              onClick={downloadModels}
              className="flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-md bg-mid-gray/10 hover:bg-mid-gray/20 text-mid-gray hover:text-text transition-colors"
              title="Download diarization models"
            >
              <Download size={13} />
              <span>Download</span>
            </button>
          )}

          {modelsReady && !enabled && (
            <CheckCircle2 size={14} className="text-green-500 shrink-0" />
          )}

          <label
            className="relative inline-flex items-center cursor-pointer"
            aria-label="Toggle diarization"
          >
            <input
              type="checkbox"
              checked={enabled}
              onChange={handleToggle}
              disabled={downloading}
              className="sr-only peer"
              aria-label="Speaker diarization"
            />
            <div className="w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-logo-primary/50 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-mid-gray/20 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-logo-primary peer-disabled:opacity-50" />
          </label>
        </div>
      </SettingContainer>

      {/* Advanced settings - only when enabled */}
      {enabled && (
        <div className="px-4 pb-3">
          <button
            type="button"
            onClick={() => setShowAdvanced(!showAdvanced)}
            className="flex items-center gap-1.5 text-[11px] text-mid-gray/50 hover:text-mid-gray/80 transition-colors"
          >
            {showAdvanced ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            <span>Advanced settings</span>
          </button>

          {showAdvanced && (
            <div className="mt-3 space-y-4 pl-1">
              {/* Max Speakers */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Max speakers</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums w-6 text-right">{maxSpeakers}</span>
                </div>
                <input
                  type="range"
                  min={2}
                  max={12}
                  step={1}
                  value={maxSpeakers}
                  onChange={(e) => updateSetting("diarization_max_speakers", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>2</span>
                  <span>12</span>
                </div>
              </div>

              {/* Similarity Threshold */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Sensitivity</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums">{threshold.toFixed(2)}</span>
                </div>
                <input
                  type="range"
                  min={0.1}
                  max={0.8}
                  step={0.05}
                  value={threshold}
                  onChange={(e) => updateSetting("diarization_threshold", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>Fewer speakers</span>
                  <span>More speakers</span>
                </div>
              </div>

              {/* Merge Gap */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Merge gap</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums">{mergeGap.toFixed(1)}s</span>
                </div>
                <input
                  type="range"
                  min={0.5}
                  max={5}
                  step={0.5}
                  value={mergeGap}
                  onChange={(e) => updateSetting("diarization_merge_gap", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>0.5s</span>
                  <span>5.0s</span>
                </div>
              </div>

              {/* Reset to defaults */}
              {!isDefault && (
                <button
                  type="button"
                  onClick={resetToDefaults}
                  className="flex items-center gap-1.5 text-[11px] text-mid-gray/40 hover:text-mid-gray/70 transition-colors mt-1"
                >
                  <RotateCcw size={11} />
                  <span>Reset to defaults</span>
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
```

