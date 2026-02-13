import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TranscriptionState {
  status: "idle" | "transcribing" | "completed" | "error" | "cancelled";
  progress: number;
  etaSeconds: number | null;
  phase: string | null;
  error: string | null;
  hasResult: boolean;
}

interface TranscriptionStatusEvent {
  recording_path: string;
  status: "started" | "completed" | "error" | "cancelled";
  error?: string;
}

interface TranscriptionProgressEvent {
  recording_path: string;
  progress?: number;
  eta_seconds?: number;
}

interface TranscriptionPhaseEvent {
  recording_path: string;
  phase: string | null;
}

interface BackendTranscriptionState {
  status: string;
  progress: number;
  eta_seconds: number | null;
  phase: string | null;
}

// --- Module-level state (survives component unmounts) ---
let transcriptionStates = new Map<string, TranscriptionState>();
const listeners = new Set<(states: Map<string, TranscriptionState>) => void>();
let didInit = false;

const notify = () => {
  const snapshot = new Map(transcriptionStates);
  listeners.forEach((l) => l(snapshot));
};

const getOrDefault = (path: string): TranscriptionState =>
  transcriptionStates.get(path) || {
    status: "idle",
    progress: 0,
    etaSeconds: null,
    phase: null,
    error: null,
    hasResult: false,
  };

const updateState = (path: string, partial: Partial<TranscriptionState>) => {
  const current = getOrDefault(path);
  transcriptionStates.set(path, { ...current, ...partial });
  notify();
};

// One-time global event listener setup
const initListeners = async () => {
  if (didInit) return;
  didInit = true;

  // Restore active states from backend
  try {
    const backendStates = await invoke<Record<string, BackendTranscriptionState>>(
      "get_all_transcription_states"
    );
    for (const [path, state] of Object.entries(backendStates)) {
      if (state.status === "started" || state.status === "transcribing") {
        updateState(path, {
          status: "transcribing",
          progress: state.progress,
          etaSeconds: state.eta_seconds ?? null,
          phase: state.phase ?? null,
          error: null,
        });
      }
    }
  } catch (e) {
    console.error("Failed to load transcription states:", e);
  }

  await listen<TranscriptionStatusEvent>("transcription-status", (event) => {
    const { recording_path, status, error } = event.payload;
    if (status === "started") {
      updateState(recording_path, {
        status: "transcribing",
        progress: 0,
        etaSeconds: null,
        phase: "preparing-audio",
        error: null,
      });
    } else if (status === "completed") {
      updateState(recording_path, {
        status: "completed",
        progress: 1,
        etaSeconds: 0,
        phase: null,
        hasResult: true,
      });
    } else if (status === "error") {
      updateState(recording_path, {
        status: "error",
        error: error ?? "Transcription failed",
        phase: null,
      });
    } else if (status === "cancelled") {
      updateState(recording_path, {
        status: "cancelled",
        progress: 0,
        etaSeconds: null,
        phase: null,
        error: null,
      });
    }
  });

  await listen<TranscriptionProgressEvent>("transcription-progress", (event) => {
    const { recording_path, progress, eta_seconds } = event.payload;
    const current = getOrDefault(recording_path);
    updateState(recording_path, {
      progress: progress ?? current.progress,
      etaSeconds: typeof eta_seconds === "number" ? eta_seconds : current.etaSeconds,
    });
  });

  await listen<TranscriptionPhaseEvent>("transcription-phase", (event) => {
    const { recording_path, phase } = event.payload;
    updateState(recording_path, { phase });
  });
};

// --- Hook ---
export function useTranscriptionProgress() {
  const [states, setStates] = useState(new Map(transcriptionStates));

  useEffect(() => {
    initListeners();
    const listener = (next: Map<string, TranscriptionState>) => {
      setStates(next);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  const startTranscription = async (path: string) => {
    try {
      await invoke("start_transcription", { recordingPath: path });
    } catch (err) {
      console.error("Failed to start transcription:", err);
      alert("Failed to start transcription. Please try again.");
    }
  };

  const cancelTranscription = async (path: string) => {
    try {
      await invoke("cancel_transcription", { recordingPath: path });
    } catch (err) {
      console.error("Failed to cancel transcription:", err);
    }
  };

  return { states, startTranscription, cancelTranscription };
}
