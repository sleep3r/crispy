// Speaker diarization. Two ONNX models (pyannote segmentation-3.0 + WeSpeaker CAM++),
// both run on the in-tree `ort` rc.12 runtime — no pyannote-rs dependency.
//
// Pipeline:
// - Binary powerset VAD (segmentation-3.0) -> continuous speech segments (silence-cut).
// - Segments chunked to ~4s, <1.5s discarded, CAM++ embedding per chunk.
// - NME-SC spectral clustering AUTO-estimates the speaker count (eigengap) — no manual
//   distance threshold and no hard max_speakers force-merge (max_speakers is an upper
//   bound on the search only).
// - Chronological speaker IDs; overlap-based word->speaker alignment downstream.

use anyhow::{bail, Context, Result};
use log::info;
use ndarray::{Array1, Axis, IxDyn};
use ort::{
    session::Session,
    value::{Tensor, TensorRef},
};
use std::path::Path;
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

/// WeSpeaker CAM++ speaker-embedding extractor on the in-tree ort rc.12 runtime.
/// Ported from pyannote-rs's 43-line EmbeddingExtractor so we no longer depend on the
/// (ort rc.10-pinned, rc.12-incompatible) crate. Same model file and tensor names
/// ("feats" in, "embs" out); features via knf-rs (kaldi-native-fbank, no ort dep).
struct EmbeddingExtractor {
    session: Session,
}

impl EmbeddingExtractor {
    fn new(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .context("ort: Session::builder failed")?
            .commit_from_file(model_path)
            .context("ort: failed to load embedding model")?;
        Ok(Self { session })
    }

    fn compute(&mut self, samples: &[i16]) -> Result<Vec<f32>> {
        let mut samples_f32 = vec![0.0f32; samples.len()];
        knf_rs::convert_integer_to_float_audio(samples, &mut samples_f32);

        // knf-rs returns an ndarray-0.16 Array2; rebuild it as an ndarray-0.17 Array3
        // (with the batch dim) via raw data, so the type matches the ndarray version that
        // ort's `ndarray` feature uses in this crate (0.17).
        let fbank =
            knf_rs::compute_fbank(&samples_f32).map_err(|e| anyhow::anyhow!("fbank: {e:?}"))?;
        let (frames, mels) = fbank.dim();
        let flat: Vec<f32> = fbank.iter().copied().collect();
        let features = ndarray::Array3::<f32>::from_shape_vec((1, frames, mels), flat)?;

        let outputs = self
            .session
            .run(ort::inputs!["feats" => Tensor::from_array(features)?])?;
        let (_shape, data) = outputs
            .get("embs")
            .context("embedding output 'embs' not found")?
            .try_extract_tensor::<f32>()?;
        Ok(data.to_vec())
    }
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

    // Guard against misconfiguration: the Phase-3 force-merge loop does a double
    // `clusters.remove()` per iteration and panics (index out of bounds) if it is
    // ever asked to merge down to fewer than 1 cluster. Clamp to a sane minimum.
    let max_speakers = max_speakers.max(1);

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

    let mut extractor = EmbeddingExtractor::new(embedding_model_path)
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
            valid_embeddings.push(embedding);
            valid_segments.push(segment);
        }
    }

    if valid_segments.is_empty() {
        return Ok(Vec::new());
    }

    // Cluster the chunk embeddings into speakers with NME-SC: spectral clustering with
    // AUTOMATIC speaker-count estimation via the normalized maximum eigengap
    // (Park et al. 2019, arXiv:2003.02405). Replaces the old AHC — there is no distance
    // threshold, and max_speakers is only an upper bound on the eigengap search.
    let n = valid_embeddings.len();
    let _ = threshold; // obsolete with spectral auto-count; kept for signature compatibility
    eprintln!(
        "[diarization] NME-SC over {} speech chunks (max_speakers <= {})",
        n, max_speakers
    );

    // Chronological speaker ID assignment (first to speak = "Speaker 1") happens below;
    // here we just get a raw label per chunk.
    let segment_labels: Vec<usize> = if n <= 2 {
        vec![0; n]
    } else {
        nme_sc(&valid_embeddings, max_speakers)
    };

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

/// Cosine similarity in [0, 1].
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    (1.0 - cosine_distance(a, b)).clamp(0.0, 1.0)
}

fn sq_dist(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

/// Keep the `p` largest neighbours per row, symmetrize (max), and return the symmetric
/// normalized graph Laplacian L = I - D^-1/2 A D^-1/2 as an N×N matrix.
fn pruned_normalized_laplacian(aff: &[Vec<f32>], p: usize) -> Vec<Vec<f32>> {
    let n = aff.len();
    let mut a = vec![vec![0.0f32; n]; n];
    for i in 0..n {
        let mut order: Vec<usize> = (0..n).filter(|&j| j != i).collect();
        order.sort_by(|&x, &y| {
            aff[i][y]
                .partial_cmp(&aff[i][x])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for &j in order.iter().take(p.min(n.saturating_sub(1))) {
            a[i][j] = aff[i][j];
        }
    }
    // symmetrize by max
    for i in 0..n {
        for j in (i + 1)..n {
            let m = a[i][j].max(a[j][i]);
            a[i][j] = m;
            a[j][i] = m;
        }
    }
    let dinv: Vec<f32> = (0..n)
        .map(|i| 1.0 / a[i].iter().sum::<f32>().max(1e-9).sqrt())
        .collect();
    let mut l = vec![vec![0.0f32; n]; n];
    for i in 0..n {
        for j in 0..n {
            let norm_a = dinv[i] * a[i][j] * dinv[j];
            l[i][j] = if i == j { 1.0 - norm_a } else { -norm_a };
        }
    }
    l
}

/// Number of speakers = position of the largest gap among the smallest eigenvalues of
/// the normalized Laplacian (k near-zero eigenvalues for k clusters). Returns (k, gap).
fn max_eigengap(evals_sorted_asc: &[f32], kmax: usize) -> (usize, f32) {
    let lim = (kmax + 1).min(evals_sorted_asc.len());
    let mut best_k = 1usize;
    let mut best_gap = f32::MIN;
    for i in 1..lim {
        let gap = evals_sorted_asc[i] - evals_sorted_asc[i - 1];
        if gap > best_gap {
            best_gap = gap;
            best_k = i;
        }
    }
    (best_k.max(1), best_gap.max(0.0))
}

/// Deterministic k-means (k-means++ farthest-point seeding) over spectral rows.
fn kmeans(points: &[Vec<f32>], k: usize) -> Vec<usize> {
    let n = points.len();
    if k <= 1 || n == 0 {
        return vec![0; n];
    }
    if k >= n {
        return (0..n).collect();
    }
    let dim = points[0].len();
    let mut centers: Vec<Vec<f32>> = vec![points[0].clone()];
    while centers.len() < k {
        let mut best_i = 0usize;
        let mut best_d = -1.0f32;
        for (i, pt) in points.iter().enumerate() {
            let d = centers.iter().map(|c| sq_dist(pt, c)).fold(f32::MAX, f32::min);
            if d > best_d {
                best_d = d;
                best_i = i;
            }
        }
        centers.push(points[best_i].clone());
    }
    let mut labels = vec![0usize; n];
    for _ in 0..50 {
        let mut changed = false;
        for (i, pt) in points.iter().enumerate() {
            let mut bc = 0usize;
            let mut bd = f32::MAX;
            for (c, ctr) in centers.iter().enumerate() {
                let d = sq_dist(pt, ctr);
                if d < bd {
                    bd = d;
                    bc = c;
                }
            }
            if labels[i] != bc {
                labels[i] = bc;
                changed = true;
            }
        }
        let mut sums = vec![vec![0.0f32; dim]; k];
        let mut counts = vec![0usize; k];
        for (i, pt) in points.iter().enumerate() {
            counts[labels[i]] += 1;
            for d in 0..dim {
                sums[labels[i]][d] += pt[d];
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                for d in 0..dim {
                    centers[c][d] = sums[c][d] / counts[c] as f32;
                }
            }
        }
        if !changed {
            break;
        }
    }
    labels
}

/// NME-SC spectral clustering with automatic speaker-count estimation
/// (Park et al. 2019, arXiv:2003.02405). Sweeps the affinity-pruning parameter p,
/// picks the p minimising (p/n)/max_eigengap, reads the speaker count k off the eigengap,
/// then runs k-means in the k-dim spectral embedding. No manual threshold; max_speakers
/// is only an upper bound on the search.
fn nme_sc(embeddings: &[Vec<f32>], max_speakers: usize) -> Vec<usize> {
    use nalgebra::{DMatrix, SymmetricEigen};
    let n = embeddings.len();
    if n == 0 {
        return vec![];
    }
    if n <= 2 {
        return vec![0; n];
    }
    let kmax = max_speakers.max(1).min(n - 1);

    // Full cosine-similarity affinity (zero diagonal).
    let mut aff = vec![vec![0.0f32; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let s = cosine_similarity(&embeddings[i], &embeddings[j]);
            aff[i][j] = s;
            aff[j][i] = s;
        }
    }

    let eigvals_for = |p: usize| -> Vec<f32> {
        let lap = pruned_normalized_laplacian(&aff, p);
        let m = DMatrix::<f32>::from_fn(n, n, |i, j| lap[i][j]);
        let mut ev: Vec<f32> = SymmetricEigen::new(m).eigenvalues.iter().copied().collect();
        ev.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ev
    };

    // Sweep p; pick the one minimising (p/n)/max_eigengap (the NME criterion).
    let p_max = (n - 1).min(((n as f64).sqrt() as usize).max(2) * 2);
    let mut best: Option<(f32, usize, usize)> = None; // (ratio, p, k)
    for p in 1..=p_max {
        let ev = eigvals_for(p);
        let (k, gap) = max_eigengap(&ev, kmax);
        let ratio = (p as f32 / n as f32) / gap.max(1e-6);
        if best.map_or(true, |(r, _, _)| ratio < r) {
            best = Some((ratio, p, k));
        }
    }
    let (_, p_star, k) = best.unwrap_or((0.0, 1, 1));
    let k = k.max(1).min(kmax);
    eprintln!("[diarization] NME-SC: p*={}, estimated speakers={}", p_star, k);
    if k <= 1 {
        return vec![0; n];
    }

    // Spectral embedding at p*: the k eigenvectors with the smallest eigenvalues.
    let lap = pruned_normalized_laplacian(&aff, p_star);
    let m = DMatrix::<f32>::from_fn(n, n, |i, j| lap[i][j]);
    let eig = SymmetricEigen::new(m);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        eig.eigenvalues[a]
            .partial_cmp(&eig.eigenvalues[b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut spectral = vec![vec![0.0f32; k]; n];
    for row in 0..n {
        for c in 0..k {
            spectral[row][c] = eig.eigenvectors[(row, idx[c])];
        }
        let norm: f32 = spectral[row].iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-9 {
            for c in 0..k {
                spectral[row][c] /= norm;
            }
        }
    }
    kmeans(&spectral, k)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // --- nme_sc (automatic speaker-count spectral clustering) ---

    /// Synthetic embeddings: each cluster points along a distinct axis (so cross-cluster
    /// cosine similarity ~ 0, within-cluster ~ 1), with tiny per-point jitter to avoid
    /// exact degeneracy.
    fn cluster_emb(centers: &[usize], per: usize, dim: usize) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        for (ci, &c) in centers.iter().enumerate() {
            for p in 0..per {
                let mut v = vec![0.0f32; dim];
                v[c] = 1.0;
                v[dim - 1] += 0.01 * (ci as f32 + 1.0) + 0.001 * p as f32;
                out.push(v);
            }
        }
        out
    }

    fn distinct(labels: &[usize]) -> usize {
        labels.iter().copied().collect::<HashSet<_>>().len()
    }

    #[test]
    fn nme_sc_detects_two_speakers() {
        let labels = nme_sc(&cluster_emb(&[0, 1], 5, 6), 8);
        assert_eq!(distinct(&labels), 2, "labels={:?}", labels);
    }

    #[test]
    fn nme_sc_detects_three_speakers() {
        let labels = nme_sc(&cluster_emb(&[0, 1, 2], 5, 6), 8);
        assert_eq!(distinct(&labels), 3, "labels={:?}", labels);
    }

    #[test]
    fn nme_sc_single_speaker() {
        let labels = nme_sc(&cluster_emb(&[0], 6, 6), 8);
        assert_eq!(distinct(&labels), 1, "labels={:?}", labels);
    }

    #[test]
    fn nme_sc_trivial_small_input() {
        assert_eq!(nme_sc(&[vec![1.0, 0.0]], 8), vec![0]);
        assert_eq!(nme_sc(&[vec![1.0, 0.0], vec![0.0, 1.0]], 8), vec![0, 0]);
    }

    #[test]
    fn nme_sc_respects_max_speakers_upper_bound() {
        // 3 real clusters but capped at 2 -> at most 2 labels.
        let labels = nme_sc(&cluster_emb(&[0, 1, 2], 5, 6), 2);
        assert!(distinct(&labels) <= 2, "labels={:?}", labels);
    }

    // --- f32_to_i16 ---

    #[test]
    fn f32_to_i16_silence() {
        assert_eq!(f32_to_i16(&[0.0, 0.0, 0.0]), vec![0, 0, 0]);
    }

    #[test]
    fn f32_to_i16_full_scale() {
        let result = f32_to_i16(&[1.0, -1.0]);
        assert_eq!(result[0], 32767);
        assert_eq!(result[1], -32767);
    }

    #[test]
    fn f32_to_i16_clamps_overflow() {
        // Values beyond [-1, 1] should be clamped
        let result = f32_to_i16(&[2.0, -3.0]);
        assert_eq!(result[0], 32767);
        assert_eq!(result[1], -32767);
    }

    #[test]
    fn f32_to_i16_half_amplitude() {
        let result = f32_to_i16(&[0.5]);
        assert_eq!(result[0], 16383);
    }

    #[test]
    fn f32_to_i16_empty() {
        assert!(f32_to_i16(&[]).is_empty());
    }

    // --- cosine_distance ---

    #[test]
    fn cosine_distance_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let d = cosine_distance(&v, &v);
        assert!(d.abs() < 0.001, "Identical vectors should have distance ~0, got {}", d);
    }

    #[test]
    fn cosine_distance_opposite_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 2.0).abs() < 0.001, "Opposite vectors should have distance ~2, got {}", d);
    }

    #[test]
    fn cosine_distance_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 0.001, "Orthogonal vectors should have distance ~1, got {}", d);
    }

    #[test]
    fn cosine_distance_zero_vector() {
        let a = vec![0.0, 0.0];
        let b = vec![1.0, 1.0];
        assert_eq!(cosine_distance(&a, &b), 1.0);
    }

    // --- merge_consecutive_segments ---

    #[test]
    fn merge_consecutive_same_speaker() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 1.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 1.1, end: 2.0, speaker: "Speaker 1".to_string() },
        ];
        let merged = merge_consecutive_segments(&segments, 0.5);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].start, 0.0);
        assert_eq!(merged[0].end, 2.0);
    }

    #[test]
    fn merge_consecutive_different_speakers() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 1.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 1.1, end: 2.0, speaker: "Speaker 2".to_string() },
        ];
        let merged = merge_consecutive_segments(&segments, 0.5);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_consecutive_gap_too_large() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 1.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 5.0, end: 6.0, speaker: "Speaker 1".to_string() },
        ];
        let merged = merge_consecutive_segments(&segments, 0.5);
        assert_eq!(merged.len(), 2, "Gap of 4s exceeds merge_gap of 0.5s");
    }

    #[test]
    fn merge_consecutive_empty() {
        let merged = merge_consecutive_segments(&[], 0.5);
        assert!(merged.is_empty());
    }

    // --- find_speaker_at_time ---

    #[test]
    fn find_speaker_exact_match() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 5.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 5.5, end: 10.0, speaker: "Speaker 2".to_string() },
        ];
        assert_eq!(find_speaker_at_time(2.5, &segments), "Speaker 1");
        assert_eq!(find_speaker_at_time(7.0, &segments), "Speaker 2");
    }

    #[test]
    fn find_speaker_boundary() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 5.0, speaker: "Speaker 1".to_string() },
        ];
        // At exact boundary
        assert_eq!(find_speaker_at_time(0.0, &segments), "Speaker 1");
        assert_eq!(find_speaker_at_time(5.0, &segments), "Speaker 1");
    }

    #[test]
    fn find_speaker_in_gap_picks_closest() {
        let segments = vec![
            SpeakerSegment { start: 0.0, end: 3.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 7.0, end: 10.0, speaker: "Speaker 2".to_string() },
        ];
        // Time 4.0 is closer to Speaker 1 (ends at 3.0, dist=1.0) than Speaker 2 (starts at 7.0, dist=3.0)
        assert_eq!(find_speaker_at_time(4.0, &segments), "Speaker 1");
        // Time 6.0 is closer to Speaker 2 (starts at 7.0, dist=1.0) than Speaker 1 (ends at 3.0, dist=3.0)
        assert_eq!(find_speaker_at_time(6.0, &segments), "Speaker 2");
    }

    // --- format_diarized_text ---

    #[test]
    fn format_diarized_text_no_speakers() {
        let text = vec![
            (0.0, 0.5, "Hello".to_string()),
            (0.5, 1.0, "world".to_string()),
        ];
        let result = format_diarized_text(&text, &[]);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn format_diarized_text_empty() {
        let result = format_diarized_text(&[], &[]);
        assert_eq!(result, "");
    }

    #[test]
    fn format_diarized_text_speaker_changes() {
        let text = vec![
            (0.0, 0.5, "Hello".to_string()),
            (0.5, 1.0, "there".to_string()),
            (5.0, 5.5, "Hi".to_string()),
        ];
        let speakers = vec![
            SpeakerSegment { start: 0.0, end: 2.0, speaker: "Speaker 1".to_string() },
            SpeakerSegment { start: 4.0, end: 7.0, speaker: "Speaker 2".to_string() },
        ];
        let result = format_diarized_text(&text, &speakers);
        assert!(result.contains("[Speaker 1|"), "Should have Speaker 1 header");
        assert!(result.contains("Hello"), "Should contain Hello");
        assert!(result.contains("[Speaker 2|"), "Should have Speaker 2 header");
        assert!(result.contains("Hi"), "Should contain Hi");
    }

    #[test]
    fn format_diarized_text_skips_empty_words() {
        let text = vec![
            (0.0, 0.5, "Hello".to_string()),
            (0.5, 1.0, "  ".to_string()),
            (1.0, 1.5, "world".to_string()),
        ];
        let speakers = vec![
            SpeakerSegment { start: 0.0, end: 2.0, speaker: "Speaker 1".to_string() },
        ];
        let result = format_diarized_text(&text, &speakers);
        assert!(result.contains("Hello"));
        assert!(result.contains("world"));
        // The "  " should be skipped
    }
}
