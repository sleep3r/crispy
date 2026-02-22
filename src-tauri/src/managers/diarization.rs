// Speaker diarization using pyannote-rs.
// Two ONNX models: segmentation + speaker embedding.
//
// Improvements over naive approach:
// - Binary VAD (Speech vs Silence) avoids false cuts on local speaker label changes.
// - State is maintained across 10s window boundaries to avoid artificial cuts.
// - Adjacent speech segments with < 0.5s gaps are merged before embeddings.
// - Segments shorter than 1.5s are filtered out to avoid noisy CAM++ embeddings.
// - Agglomerative Hierarchical Clustering (AHC) handles global speaker matching.
// - Adaptive noise filtering prevents deleting valid rare speakers.
// - Overlap-based text alignment assigns un-embedded short words dynamically.

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
/// Cuts ONLY on silence, merging local speaker labels into continuous speech segments.
fn pyannote_get_segments_fixed(
    samples: &[i16],
    sample_rate: u32,
    segmentation_model_path: &std::path::Path,
    merge_gap_seconds: f64,
) -> Result<Vec<VadSegment>> {
    if sample_rate != 16_000 {
        bail!(
            "pyannote segmentation expects 16kHz mono. Got {} Hz.",
            sample_rate
        );
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

    // Pad audio to a multiple of window_size, plus one extra window to catch trailing speech
    let mut padded: Vec<i16> = Vec::with_capacity(samples.len() + window_size * 2);
    padded.extend_from_slice(samples);
    let rem = padded.len() % window_size;
    if rem != 0 {
        padded.extend(std::iter::repeat(0i16).take(window_size - rem));
    }
    padded.extend(std::iter::repeat(0i16).take(window_size));

    let mut raw_segments: Vec<(usize, usize)> = Vec::new();
    let mut win_start = 0usize;

    // State maintained across overlapping/adjacent windows
    let mut current_is_speech = false;
    let mut current_speech_start_idx = 0usize;

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
        let outputs =
            session.run(ort::inputs![TensorRef::from_array_view(input.view().into_dyn())?])?;

        let (shape, data) = outputs[0].try_extract_tensor::<f32>()?;
        let shape_vec: Vec<usize> = (0..shape.len()).map(|i| shape[i] as usize).collect();
        let view = ndarray::ArrayViewD::<f32>::from_shape(IxDyn(&shape_vec), data)?;

        let frames = if shape_vec.len() == 3 {
            view.index_axis(Axis(0), 0)
        } else {
            view
        };
        let _classes = frames.shape()[1];
        let mut local_labels = Vec::with_capacity(frames.shape()[0]);

        // 1. Decode Powerset - we only care if it's silence or speech
        for probs in frames.axis_iter(Axis(0)) {
            let max_val = probs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mut sum_exp = 0.0;
            let mut p_sil = 0.0;
            for (i, &v) in probs.iter().enumerate() {
                let e = (v - max_val).exp();
                if i == 0 {
                    p_sil = e; // In Pyannote Powerset, index 0 is always silence.
                }
                sum_exp += e;
            }
            p_sil /= sum_exp;

            let label = if p_sil > 0.5 { 0u8 } else { 1u8 };
            local_labels.push(label);
        }

        // 2. Median filter (~180ms) to remove micro-glitches
        let window_len = 11;
        let half = window_len / 2;
        let mut smoothed = vec![0u8; local_labels.len()];
        for i in 0..local_labels.len() {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(local_labels.len());
            let mut count_speech = 0;
            let mut count_total = 0;
            for &l in &local_labels[start..end] {
                if l == 1 {
                    count_speech += 1;
                }
                count_total += 1;
            }
            smoothed[i] = if count_speech > count_total / 2 {
                1
            } else {
                0
            };
        }

        // 3. Build contiguous speech boundaries (ignoring local speaker changes)
        for (i, &label) in smoothed.iter().enumerate() {
            let is_speech = label == 1;
            if is_speech != current_is_speech {
                let sample_idx = win_start + frame_start + i * frame_step;
                if is_speech {
                    // Start of speech
                    // Snap to 0 if it's the very beginning of the file (first 100ms)
                    current_speech_start_idx = if sample_idx < 1600 { 0 } else { sample_idx };
                } else {
                    // End of speech
                    let end_idx = sample_idx;
                    let s_idx = current_speech_start_idx.min(samples.len());
                    let e_idx = end_idx.min(samples.len());
                    if e_idx > s_idx {
                        raw_segments.push((s_idx, e_idx));
                    }
                }
                current_is_speech = is_speech;
            }
        }

        win_start += window_size;
    }

    // Close any trailing speech
    if current_is_speech {
        let e_idx = samples.len();
        let s_idx = current_speech_start_idx.min(samples.len());
        if e_idx > s_idx {
            raw_segments.push((s_idx, e_idx));
        }
    }

    // 4. Merge close segments (e.g. breaths / stutters)
    raw_segments.sort_by_key(|&(s, _)| s);

    let mut merged_indices: Vec<(usize, usize)> = Vec::new();
    let merge_gap_samples = (sample_rate as f64 * merge_gap_seconds) as usize;
    let min_dur_samples = (sample_rate as f64 * 1.5) as usize; // Minimum valid duration: 1.5s

    for (start_idx, end_idx) in raw_segments {
        if let Some(last) = merged_indices.last_mut() {
            if start_idx <= last.1 + merge_gap_samples {
                last.1 = last.1.max(end_idx);
            } else {
                merged_indices.push((start_idx, end_idx));
            }
        } else {
            merged_indices.push((start_idx, end_idx));
        }
    }

    let mut out: Vec<VadSegment> = Vec::new();
    for (start_idx, end_idx) in &merged_indices {
        // Discard short segments that produce unreliable embeddings
        if end_idx.saturating_sub(*start_idx) >= min_dur_samples {
            out.push(VadSegment {
                start: *start_idx as f64 / sample_rate as f64,
                end: *end_idx as f64 / sample_rate as f64,
                samples: samples[*start_idx..*end_idx].to_vec(),
            });
        }
    }

    // Fallback: If ALL segments were discarded but someone actually spoke, keep the longest one
    if out.is_empty() && !merged_indices.is_empty() {
        let longest = merged_indices
            .iter()
            .max_by_key(|i| i.1.saturating_sub(i.0))
            .unwrap();
        out.push(VadSegment {
            start: longest.0 as f64 / sample_rate as f64,
            end: longest.1 as f64 / sample_rate as f64,
            samples: samples[longest.0..longest.1].to_vec(),
        });
    }

    eprintln!(
        "[diarization] segmentation complete: {} merged segments found (>1.5s)",
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
        pyannote_get_segments_fixed(samples_i16, sample_rate, segmentation_model_path, merge_gap)?;
    if segments.is_empty() {
        return Ok(Vec::new());
    }

    let mut extractor = EmbeddingExtractor::new(embedding_model_path.to_str().unwrap_or(""))
        .map_err(|e| anyhow::anyhow!("Failed to load embedding model: {:?}", e))?;

    // Chunk long monologues into ~4 second parts.
    // This allows CAM++ to output sharp vectors, and independent clustering guarantees
    // dynamic resolution. If it's the same speaker, AHC merges them back seamlessly.
    let mut chunked_segments = Vec::new();
    let max_dur = 4.0;

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
        "[diarization] AHC: {} valid speech chunks, threshold={}, max_speakers={}",
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

    // Phase 1: Merge clusters while min distance is below threshold
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

        if min_dist > threshold as f32 {
            eprintln!(
                "[diarization] AHC stopped at {} clusters (min_dist={:.4} > threshold={:.4})",
                clusters.len(),
                min_dist,
                threshold
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

    // Phase 2: Handle Outliers (Tiny clusters)
    // ONLY delete tiny clusters if we have exceeded max_speakers.
    // This preserves real speakers who simply didn't speak a lot.
    if clusters.len() > max_speakers {
        let mut min_cluster_size = (n as f64 * 0.02).ceil() as usize;
        min_cluster_size = min_cluster_size.max(2);

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
                let avg_dist: f32 = cluster
                    .iter()
                    .map(|&c| dist_matrix[idx][c])
                    .sum::<f32>()
                    / cluster.len() as f32;
                if avg_dist < best_dist {
                    best_dist = avg_dist;
                    best_cluster = ci;
                }
            }
            real_clusters[best_cluster].push(idx);
        }
        clusters = real_clusters;
    }

    // Phase 3: Force merge down to max_speakers if still exceeding
    while clusters.len() > max_speakers {
        let mut min_dist = f32::MAX;
        let mut merge_pair = (0, 0);
        let k = clusters.len();
        for i in 0..k {
            for j in (i + 1)..k {
                let mut sum_dist = 0.0;
                for &u in &clusters[i] {
                    for &v in &clusters[j] {
                        sum_dist += dist_matrix[u][v];
                    }
                }
                let avg_dist = sum_dist / (clusters[i].len() * clusters[j].len()) as f32;
                if avg_dist < min_dist {
                    min_dist = avg_dist;
                    merge_pair = (i, j);
                }
            }
        }
        let (i, j) = merge_pair;
        let mut merged = clusters[i].clone();
        merged.extend(clusters[j].iter().copied());
        clusters.remove(j);
        clusters.remove(i);
        clusters.push(merged);
    }

    // Log cluster distribution
    for (ci, cluster) in clusters.iter().enumerate() {
        eprintln!(
            "[diarization] cluster {}: {} segments ({:.1}%)",
            ci,
            cluster.len(),
            cluster.len() as f64 / n as f64 * 100.0
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

    // Final merge passes output identical contiguous blocks
    let merged = merge_consecutive_segments(&result, merge_gap);

    eprintln!(
        "[diarization] complete: {} clusters, {} merged text segments",
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
            // max(0.0) safely handles slightly overlapping segment boundaries
            let gap = (seg.start - last.end).max(0.0);
            if last.speaker == seg.speaker && gap <= merge_gap {
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
            if !current_words.is_empty() {
                lines.push(current_words.join(" "));
                current_words.clear();
            }
            current_speaker = Some(speaker.clone());
            lines.push(format!("\n[{}|{:.1}]", speaker, start));
        }

        current_words.push(trimmed.to_string());
    }

    if !current_words.is_empty() {
        lines.push(current_words.join(" "));
    }

    lines.join("\n").trim().to_string()
}

/// Find which speaker is active at a given time point.
fn find_speaker_at_time(time: f64, segments: &[SpeakerSegment]) -> String {
    for seg in segments {
        if time >= seg.start && time <= seg.end {
            return seg.speaker.clone();
        }
    }

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
