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
use std::collections::HashMap;
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

/// Smart text alignment: finds the dominant speaker by overlap area,
/// solving the problem of 30-second transcription chunks containing multiple speakers.
pub fn format_diarized_text(
    text_chunks: &[(f64, String)],
    speaker_segments: &[SpeakerSegment],
) -> String {
    if speaker_segments.is_empty() || text_chunks.is_empty() {
        return text_chunks
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_speaker: Option<String> = None;

    for i in 0..text_chunks.len() {
        let (start_time, text) = &text_chunks[i];
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let end_time = if i + 1 < text_chunks.len() {
            text_chunks[i + 1].0
        } else {
            *start_time + 5.0 // Tail heuristic
        };

        let speaker = find_dominant_speaker(*start_time, end_time, speaker_segments);

        if current_speaker.as_deref() != Some(speaker.as_str()) {
            current_speaker = Some(speaker.clone());
            lines.push(format!("\n[{}|{:.1}]", speaker, start_time));
        }
        lines.push(trimmed.to_string());
    }

    lines.join("\n").trim().to_string()
}

fn find_dominant_speaker(start: f64, end: f64, segments: &[SpeakerSegment]) -> String {
    let mut durations: HashMap<&str, f64> = HashMap::new();

    for seg in segments {
        let overlap_start = start.max(seg.start);
        let overlap_end = end.min(seg.end);
        if overlap_start < overlap_end {
            *durations.entry(seg.speaker.as_str()).or_insert(0.0) += overlap_end - overlap_start;
        }
    }

    if let Some((speaker, _)) = durations
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    {
        return speaker.to_string();
    }

    // If chunk falls into complete silence, snap to the closest segment
    let mid = (start + end) / 2.0;
    let mut closest = "Speaker ?".to_string();
    let mut min_dist = f64::MAX;

    for seg in segments {
        let dist = if mid < seg.start {
            seg.start - mid
        } else if mid > seg.end {
            mid - seg.end
        } else {
            0.0
        };
        if dist < min_dist {
            min_dist = dist;
            closest = seg.speaker.clone();
        }
    }
    closest
}
