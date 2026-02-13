# Transcription Progress Persistence & Cancellation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make transcription progress persist across tab switches and allow users to cancel in-progress transcriptions.

**Architecture:** Global pub/sub hook (like `useSettings`) for frontend state persistence; `AtomicBool` cooperative cancellation in Rust backend; new `cancel_transcription` Tauri command.

**Tech Stack:** React hooks, Tauri events, Rust `std::sync::atomic`

---

### Task 1: Add cancellation support to `TranscriptionManager` (backend)

**Files:**
- Modify: `src-tauri/src/managers/transcription.rs`

**Step 1: Add `cancel_flags` field and methods**

In `TranscriptionManager`, add:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

Add field to struct:
```rust
pub struct TranscriptionManager {
    engine: Mutex<Option<LoadedEngine>>,
    current_model_id: Mutex<Option<String>>,
    state: Mutex<HashMap<String, TranscriptionState>>,
    model_manager: std::sync::Arc<ModelManager>,
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}
```

In `new()`, add:
```rust
cancel_flags: Mutex::new(HashMap::new()),
```

Add three methods:
```rust
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
```

Also add a method to get all active states (needed by frontend on init):
```rust
pub fn get_all_states(&self) -> HashMap<String, TranscriptionState> {
    self.state.lock().unwrap().clone()
}
```

---

### Task 2: Add cancellation checks to `run_transcription` and new command

**Files:**
- Modify: `src-tauri/src/commands/transcription.rs`
- Modify: `src-tauri/src/main.rs` (register new command)

**Step 1: Thread cancellation flag through `start_transcription`**

In `start_transcription`, before `std::thread::spawn`:
```rust
let cancel_flag = tm.create_cancel_flag(&recording_path);
```

Pass `cancel_flag` into the spawned thread closure. After `run_transcription` returns (regardless of result), clean up the flag:
```rust
std::thread::spawn(move || {
    let result = run_transcription(&app_clone, &path_clone, &tm, &sel, &cancel_flag);
    tm.remove_cancel_flag(&path_clone);
    // ... existing status emit logic, but also handle cancelled:
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
    // ... rest unchanged
});
```

**Step 2: Add `cancel_flag` parameter to `run_transcription`**

Change signature:
```rust
fn run_transcription(
    app: &AppHandle,
    recording_path: &str,
    tm: &TranscriptionManager,
    selected_model: &Arc<std::sync::Mutex<String>>,
    cancel_flag: &AtomicBool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
```

**Step 3: Check flag in the processing loop**

Inside `process_pending` closure, at the start of the `while` loop, add:
```rust
if cancel_flag.load(Ordering::Relaxed) {
    return Ok(());
}
```

Also check before the final `pending_16k` processing and before diarization:
```rust
if cancel_flag.load(Ordering::Relaxed) {
    return Ok(());
}
```

**Step 4: Add `cancel_transcription` command**

```rust
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
```

**Step 5: Add `get_all_transcription_states` command**

```rust
#[tauri::command]
pub async fn get_all_transcription_states(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<std::collections::HashMap<String, TranscriptionState>, String> {
    Ok(transcription_manager.inner().get_all_states())
}
```

**Step 6: Register both commands in `main.rs`**

Add to `invoke_handler` list:
```rust
commands::transcription::cancel_transcription,
commands::transcription::get_all_transcription_states,
```

---

### Task 3: Create `useTranscriptionProgress` hook (frontend)

**Files:**
- Create: `src/hooks/useTranscriptionProgress.ts`

```typescript
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

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

// Backend state shape from get_all_transcription_states
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
```

---

### Task 4: Refactor `RecordingsHistory` to use the new hook

**Files:**
- Modify: `src/components/settings/recordings/RecordingsHistory.tsx`

**Changes:**

1. Add import: `import { useTranscriptionProgress } from "../../../hooks/useTranscriptionProgress";`
2. Remove: `TranscriptionStatusEvent`, `TranscriptionProgressEvent`, `TranscriptionPhaseEvent` interfaces (moved to hook)
3. Remove: `TranscriptionState` interface (moved to hook)
4. Remove: `transcriptionStates` useState and all three `useTauriListen` calls
5. Remove: `transcribeRecording` function (replaced by hook)
6. Add at top of component: `const { states: transcriptionStates, startTranscription, cancelTranscription } = useTranscriptionProgress();`
7. Update `RecordingEntry` props: add `onCancel: () => void`
8. Pass `onCancel={() => cancelTranscription(recording.path)}` and `onTranscribe={() => startTranscription(recording.path)}`
9. In `deleteRecording`: remove the `setTranscriptionStates` call (state is now in the hook — deletion from it is optional since the recording won't be rendered)

---

### Task 5: Update `RecordingEntry` UI for cancellation

**Files:**
- Modify: `src/components/settings/recordings/RecordingsHistory.tsx`

**Changes to `RecordingEntry`:**

1. Add `Square` to lucide imports (for stop/cancel icon)
2. Add `onCancel` prop to `RecordingEntryProps`
3. Handle `"cancelled"` status same as `"idle"` for display purposes
4. Replace the disabled "Transcribing..." button with a cancel button when `status === "transcribing"`:

```tsx
{status === "transcribing" ? (
  <button
    type="button"
    onClick={onCancel}
    className="flex-1 px-3 py-2 text-sm rounded-md border border-red-500/30 bg-red-500/5 hover:bg-red-500/10 text-red-600 dark:text-red-400 transition-colors"
  >
    <span className="flex items-center justify-center gap-2">
      <Square className="w-3.5 h-3.5 fill-current" />
      Cancel
    </span>
  </button>
) : (
  <button
    type="button"
    onClick={onTranscribe}
    className="flex-1 px-3 py-2 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
  >
    Transcribe
  </button>
)}
```

---

### Task 6: Build & verify

**Step 1:** Run `make build` (or the project's build command) and fix any compile errors.

**Step 2:** Manual test:
- Start a transcription, switch tabs, switch back — progress should still be visible
- Start a transcription, click Cancel — should stop and return to "Transcribe" button
- Start multiple transcriptions — each should track independently
