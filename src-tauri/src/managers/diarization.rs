// Speaker diarization using pyannote-rs.
// Two ONNX models: segmentation + speaker embedding.

use anyhow::{bail, Context, Result};
use log::info;
use ndarray::{Array1, Axis, IxDyn};
use ort::{session::Session, value::TensorRef};
use pyannote_rs::{EmbeddingExtractor, EmbeddingManager};
use std::path::PathBuf;

/// A speaker-labelled segment of audio.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpeakerSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

/// Fixed pyannote segmentation that works around bugs in pyannote-rs v0.3
#[derive(Debug, Clone)]
struct VadSegment {
    start: f64,
    end: f64,
    samples: Vec<i16>,
}

/// Fixed pyannote segmentation-3.0 VAD-style segmentation.
///
/// Fixes two bugs in pyannote_rs::get_segments():
/// 1. Iterator terminates after first 10s window if no segments found
/// 2. i16->f32 conversion doesn't normalize to [-1, 1]
///
/// Requirements:
/// - `samples` must be mono PCM i16
/// - `sample_rate` must be 16000
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

    eprintln!("[diarization] pyannote_get_segments_fixed: starting segmentation");

    // ONNX session
    let mut session = Session::builder()
        .context("ort: Session::builder failed")?
        .commit_from_file(segmentation_model_path)
        .with_context(|| format!("ort: failed to load model {:?}", segmentation_model_path))?;

    // Constants from pyannote reference implementation
    let frame_size: usize = 270;
    let frame_start: usize = 721;
    let window_size: usize = (sample_rate as usize) * 10; // 10 seconds @ 16k = 160_000

    // Pad with an extra 10s of silence so an "open" segment can close
    let mut padded: Vec<i16> = Vec::with_capacity(samples.len() + window_size);
    padded.extend_from_slice(samples);

    // pad to next multiple of window_size
    let rem = padded.len() % window_size;
    if rem != 0 {
        padded.extend(std::iter::repeat(0i16).take(window_size - rem));
    }
    // plus one extra window of zeros to flush trailing speech
    padded.extend(std::iter::repeat(0i16).take(window_size));

    let mut out: Vec<VadSegment> = Vec::new();

    // State across windows
    let mut is_speaking = false;
    let mut offset = frame_start; // in samples
    let mut start_offset = 0usize;

    // Walk windows
    let mut win_start = 0usize;
    let num_windows = (padded.len() + window_size - 1) / window_size;
    eprintln!("[diarization] processing {} windows of 10s each", num_windows);

    while win_start < padded.len() {
        let win_end = (win_start + window_size).min(padded.len());
        let window_i16 = &padded[win_start..win_end];

        // FIX: i16 -> f32 normalized [-1, 1] (not just "as f32")
        let mut window_f32 = vec![0f32; window_i16.len()];
        for (src, dst) in window_i16.iter().zip(window_f32.iter_mut()) {
            *dst = *src as f32 / 32768.0;
        }

        // shape: [1, 1, T]
        let input = Array1::from(window_f32)
            .insert_axis(Axis(0))
            .insert_axis(Axis(1));
        let input_view = input.view().into_dyn();

        let outputs = session
            .run(ort::inputs![TensorRef::from_array_view(input_view)?])
            .context("ort: session.run failed")?;

        // Take first output tensor
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("ort: failed to extract output tensor<f32>")?;

        let shape_vec: Vec<usize> = (0..shape.len()).map(|i| shape[i] as usize).collect();
        let view = ndarray::ArrayViewD::<f32>::from_shape(IxDyn(&shape_vec), data)
            .context("output: invalid shape")?;

        // expected: [B, frames, classes] so take batch 0 => [frames, classes]
        let frames = view.index_axis(Axis(0), 0);

        for probs in frames.axis_iter(Axis(0)) {
            // argmax
            let mut best_i = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for (i, &v) in probs.iter().enumerate() {
                if v > best_v {
                    best_v = v;
                    best_i = i;
                }
            }

            if best_i != 0 {
                if !is_speaking {
                    start_offset = offset;
                    is_speaking = true;
                }
            } else if is_speaking {
                let start_s = start_offset as f64 / sample_rate as f64;
                let end_s = offset as f64 / sample_rate as f64;

                // Clamp sample indices to ORIGINAL (non-padded) audio
                let start_idx = start_offset.min(samples.len().saturating_sub(1));
                let end_idx = offset.min(samples.len());

                if end_idx > start_idx {
                    eprintln!("[diarization] segment found: {:.2}s - {:.2}s ({} samples)", 
                             start_s, end_s, end_idx - start_idx);
                    out.push(VadSegment {
                        start: start_s,
                        end: end_s,
                        samples: samples[start_idx..end_idx].to_vec(),
                    });
                }

                is_speaking = false;
            }

            offset += frame_size;
        }

        win_start += window_size;
    }

    eprintln!("[diarization] pyannote_get_segments_fixed: {} segments found", out.len());

    Ok(out)
}

/// Run speaker diarization on 16 kHz mono i16 samples.
/// Returns a list of segments with speaker labels.
///
/// IMPORTANT: This function REQUIRES 16kHz mono audio.
/// Pass resampled audio from transcription pipeline, not raw WAV samples.
pub fn run_diarization(
    samples_i16: &[i16],
    sample_rate: u32,
    segmentation_model_path: &PathBuf,
    embedding_model_path: &PathBuf,
    max_speakers: usize,
) -> Result<Vec<SpeakerSegment>> {
    // Enforce 16kHz requirement
    if sample_rate != 16_000 {
        bail!("Diarization requires 16kHz mono input; got {} Hz. Use resampled audio from transcription.", sample_rate);
    }

    let duration_secs = samples_i16.len() as f64 / sample_rate as f64;
    let min_val = samples_i16.iter().copied().min().unwrap_or(0);
    let max_val = samples_i16.iter().copied().max().unwrap_or(0);
    let rms = if !samples_i16.is_empty() {
        let sum_sq: f64 = samples_i16.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum_sq / samples_i16.len() as f64).sqrt()
    } else {
        0.0
    };
    eprintln!(
        "[diarization] input: {} samples, {}Hz, {:.1}s, min={}, max={}, rms={:.0}",
        samples_i16.len(), sample_rate, duration_secs, min_val, max_val, rms
    );

    eprintln!("[diarization] using fixed segmentation (bypasses pyannote-rs bugs)");
    let segments = pyannote_get_segments_fixed(samples_i16, sample_rate, segmentation_model_path)?;
    
    eprintln!("[diarization] loading embedding model: {:?}", embedding_model_path);
    let mut extractor = EmbeddingExtractor::new(
        embedding_model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding model path"))?,
    )
    .map_err(|e| anyhow::anyhow!("Failed to load embedding model: {:?}", e))?;

    let mut manager = EmbeddingManager::new(max_speakers);
    let mut result: Vec<SpeakerSegment> = Vec::new();

    for (idx, segment) in segments.iter().enumerate() {
        eprintln!(
            "[diarization] segment #{}: start={:.2}s end={:.2}s samples={}",
            idx + 1, segment.start, segment.end, segment.samples.len()
        );
        
        let speaker = match extractor.compute(&segment.samples) {
            Ok(embedding) => {
                let emb = embedding.collect();
                if manager.get_all_speakers().len() >= max_speakers {
                    manager
                        .get_best_speaker_match(emb)
                        .map(|s| format!("Speaker {}", s + 1))
                        .unwrap_or_else(|_| "Speaker ?".to_string())
                } else {
                    let id = manager.search_speaker(emb, 0.5);
                    match id {
                        Some(s) => format!("Speaker {}", s + 1),
                        None => {
                            // New speaker was auto-added
                            let count = manager.get_all_speakers().len();
                            format!("Speaker {}", count)
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[diarization] embedding error for segment #{}: {:?}", idx + 1, e);
                "Speaker ?".to_string()
            }
        };

        eprintln!("[diarization] segment #{} -> {}", idx + 1, speaker);
        result.push(SpeakerSegment {
            start: segment.start,
            end: segment.end,
            speaker,
        });
    }

    eprintln!(
        "[diarization] segmentation complete: {} total segments",
        result.len()
    );

    // Merge consecutive segments by the same speaker
    let merged = merge_consecutive_segments(&result);
    info!("Diarization complete: {} segments", merged.len());
    Ok(merged)
}

/// Merge consecutive segments that have the same speaker label.
fn merge_consecutive_segments(segments: &[SpeakerSegment]) -> Vec<SpeakerSegment> {
    if segments.is_empty() {
        return Vec::new();
    }
    let mut merged: Vec<SpeakerSegment> = Vec::new();
    for seg in segments {
        if let Some(last) = merged.last_mut() {
            if last.speaker == seg.speaker && (seg.start - last.end).abs() < 0.5 {
                last.end = seg.end;
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
        .map(|&s| {
            let clamped = s.clamp(-1.0, 1.0);
            (clamped * 32767.0) as i16
        })
        .collect()
}

/// Format diarized transcription: combine speaker labels with transcription text chunks.
/// Each chunk has a start time (in seconds). We match each chunk to the speaker active at that time.
pub fn format_diarized_text(
    text_chunks: &[(f64, String)], // (start_time_seconds, text)
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

    for (start_time, text) in text_chunks {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Find the speaker for this chunk's start time
        let speaker = find_speaker_at(*start_time, speaker_segments);

        if current_speaker.as_ref() != Some(&speaker) {
            current_speaker = Some(speaker.clone());
            lines.push(format!("\n[{}]", speaker));
        }
        lines.push(trimmed.to_string());
    }

    lines.join("\n").trim().to_string()
}

fn find_speaker_at(time: f64, segments: &[SpeakerSegment]) -> String {
    // Find the segment that contains this time point
    for seg in segments {
        if time >= seg.start && time <= seg.end {
            return seg.speaker.clone();
        }
    }
    // If no exact match, find the closest segment
    let mut closest: Option<&SpeakerSegment> = None;
    let mut min_dist = f64::MAX;
    for seg in segments {
        let dist = if time < seg.start {
            seg.start - time
        } else {
            time - seg.end
        };
        if dist < min_dist {
            min_dist = dist;
            closest = Some(seg);
        }
    }
    closest
        .map(|s| s.speaker.clone())
        .unwrap_or_else(|| "Speaker ?".to_string())
}
