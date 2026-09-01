//! Adapter exposing a transcribe-cpp model through transcribe-rs's `SpeechModel`
//! trait, so the rest of the transcription manager stays engine-agnostic.
//!
//! transcribe-cpp covers the whole GGML/GGUF family (Whisper, Parakeet, GigaAM,
//! Canary, Voxtral, Qwen3-ASR, …) behind one loader that auto-detects the
//! architecture from the file, which is why a single `EngineType::TranscribeCpp`
//! replaces the old Whisper-only arm.
//!
//! NOTE: transcribe-cpp and transcribe-rs's `whisper-cpp` feature must never be
//! enabled together — both vendor their own ggml, the `links` keys differ so
//! cargo permits it, and the surviving copy silently misinterprets ~46 of ~100
//! graph opcodes at inference time. See the `no-whisper-rs` CI check.

use transcribe_cpp::{Model, RunOptions, Session, Task, TimestampKind};
use transcribe_rs::{
    ModelCapabilities, SpeechModel, TranscribeError, TranscribeOptions, TranscriptionResult,
    TranscriptionSegment,
};

const CAPABILITIES: ModelCapabilities = ModelCapabilities {
    name: "transcribe.cpp",
    engine_id: "transcribe_cpp",
    sample_rate: 16_000,
    // Left empty deliberately: the real language list is per-model and only known
    // after loading, while this trait wants a &'static. Callers that need it should
    // ask the model, not the trait.
    languages: &[],
    supports_timestamps: true,
    supports_translation: true,
    supports_streaming: false,
};

/// A loaded GGML/GGUF model plus its inference session.
///
/// The session holds an `Arc` to the native model, so keeping it alive is enough
/// to keep the weights alive.
pub struct TranscribeCppEngine {
    session: Session,
    /// Longest audio this model accepts in one call, in samples at 16 kHz.
    /// `None` means no practical limit. GigaAM caps at 25 s, which is shorter
    /// than our default 30 s chunk, so the caller must honour this.
    max_chunk_samples: Option<usize>,
}

impl TranscribeCppEngine {
    pub fn load(path: &std::path::Path) -> Result<Self, TranscribeError> {
        let model = Model::load(path)
            .map_err(|e| TranscribeError::Inference(format!("transcribe-cpp load: {e}")))?;
        let max_audio_ms = model.capabilities().max_audio_ms;
        let session = model
            .session()
            .map_err(|e| TranscribeError::Inference(format!("transcribe-cpp session: {e}")))?;
        Ok(Self {
            session,
            max_chunk_samples: (max_audio_ms > 0)
                .then(|| (max_audio_ms as usize) * SAMPLE_RATE / 1000),
        })
    }

    pub fn max_chunk_samples(&self) -> Option<usize> {
        self.max_chunk_samples
    }
}

const SAMPLE_RATE: usize = 16_000;

impl SpeechModel for TranscribeCppEngine {
    fn capabilities(&self) -> ModelCapabilities {
        CAPABILITIES
    }

    fn transcribe_raw(
        &mut self,
        samples: &[f32],
        options: &TranscribeOptions,
    ) -> Result<TranscriptionResult, TranscribeError> {
        let run = RunOptions {
            task: if options.translate {
                Task::Translate
            } else {
                Task::Transcribe
            },
            // Ask for the richest alignment the family supports. Whisper through
            // transcribe-cpp yields word rows, which the old whisper-rs backend
            // could not produce at all — diarization alignment improves as a
            // side effect.
            timestamps: TimestampKind::Auto,
            language: options.language.clone(),
            target_language: options.translate.then(|| "en".to_string()),
            ..RunOptions::default()
        };

        let transcript = self
            .session
            .run(samples, &run)
            .map_err(|e| TranscribeError::Inference(format!("transcribe-cpp run: {e}")))?;

        // Prefer word rows, fall back to segments; either becomes the caller's
        // timestamped output. `None` (rather than an empty Vec) tells the caller
        // to synthesize a single whole-chunk segment.
        let segments: Vec<TranscriptionSegment> = if !transcript.words.is_empty() {
            transcript
                .words
                .iter()
                .map(|w| TranscriptionSegment {
                    start: w.t0_ms as f32 / 1000.0,
                    end: w.t1_ms as f32 / 1000.0,
                    text: w.text.clone(),
                })
                .collect()
        } else {
            transcript
                .segments
                .iter()
                .map(|s| TranscriptionSegment {
                    start: s.t0_ms as f32 / 1000.0,
                    end: s.t1_ms as f32 / 1000.0,
                    text: s.text.clone(),
                })
                .collect()
        };

        Ok(TranscriptionResult {
            text: transcript.text,
            segments: (!segments.is_empty()).then_some(segments),
        })
    }
}
