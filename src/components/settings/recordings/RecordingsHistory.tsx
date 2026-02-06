import React, { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Trash2, FileText, Loader2, ExternalLink } from "lucide-react";
import { AudioPlayer } from "../../ui/AudioPlayer";
import { useTauriListen } from "../../../hooks/useTauriListen";

interface RecordingFile {
  name: string;
  path: string;
  size: number;
  created: number;
  duration_seconds?: number | null;
}

interface TranscriptionStatusEvent {
  recording_path: string;
  status: "started" | "completed" | "error";
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

// Centralized transcription state per recording
interface TranscriptionState {
  status: "idle" | "transcribing" | "completed" | "error";
  progress: number;
  etaSeconds: number | null;
  phase: string | null;
  error: string | null;
  hasResult: boolean;
}

const formatFileSize = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
};

const formatDate = (timestamp: number): string => {
  const date = new Date(timestamp * 1000);
  return date.toLocaleString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
};

const formatEta = (seconds: number | null, phase: string | null): string => {
  if (phase === "preparing-audio") return "Preparing audio...";
  if (phase === "transcribing") {
    if (seconds == null) return "Transcribing...";
    if (seconds < 1) return "Finishing...";
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return mins > 0 ? `~${mins}m ${secs}s left` : `~${secs}s left`;
  }
  if (phase === "diarizing") return "Identifying speakers...";
  return "Processing...";
};

export const RecordingsHistory: React.FC = () => {
  const [recordings, setRecordings] = useState<RecordingFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentlyPlayingPath, setCurrentlyPlayingPath] = useState<string | null>(null);

  // Centralized transcription state: Map<recording_path, TranscriptionState>
  const [transcriptionStates, setTranscriptionStates] = useState<Map<string, TranscriptionState>>(
    new Map()
  );

  const initialLoadDone = useRef(false);

  const loadRecordings = useCallback(async () => {
    try {
      // Only show loading spinner on initial load, not on refresh
      if (!initialLoadDone.current) {
        setLoading(true);
      }
      setError(null);
      const result = await invoke<RecordingFile[]>("get_recordings");
      setRecordings(result);
      initialLoadDone.current = true;
    } catch (err) {
      console.error("Failed to load recordings:", err);
      setError(err instanceof Error ? err.message : "Failed to load recordings");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadRecordings();
  }, [loadRecordings]);

  // Centralized Tauri listeners (ONE per event type, not per recording)
  useTauriListen<TranscriptionStatusEvent>("transcription-status", (event) => {
    const { recording_path, status, error } = event.payload;
    setTranscriptionStates((prev) => {
      const newMap = new Map(prev);
      const current = newMap.get(recording_path) || {
        status: "idle",
        progress: 0,
        etaSeconds: null,
        phase: null,
        error: null,
        hasResult: false,
      };

      if (status === "started") {
        newMap.set(recording_path, {
          ...current,
          status: "transcribing",
          progress: 0,
          etaSeconds: null,
          phase: "preparing-audio",
          error: null,
        });
      } else if (status === "completed") {
        newMap.set(recording_path, {
          ...current,
          status: "completed",
          progress: 1,
          etaSeconds: 0,
          phase: null,
          hasResult: true,
        });
      } else if (status === "error") {
        newMap.set(recording_path, {
          ...current,
          status: "error",
          error: error ?? "Transcription failed",
          phase: null,
        });
      }
      return newMap;
    });
  });

  useTauriListen<TranscriptionProgressEvent>("transcription-progress", (event) => {
    const { recording_path, progress, eta_seconds } = event.payload;
    setTranscriptionStates((prev) => {
      const newMap = new Map(prev);
      const current = newMap.get(recording_path);
      if (current) {
        newMap.set(recording_path, {
          ...current,
          progress: progress ?? current.progress,
          etaSeconds: typeof eta_seconds === "number" ? eta_seconds : current.etaSeconds,
        });
      }
      return newMap;
    });
  });

  useTauriListen<TranscriptionPhaseEvent>("transcription-phase", (event) => {
    const { recording_path, phase } = event.payload;
    setTranscriptionStates((prev) => {
      const newMap = new Map(prev);
      const current = newMap.get(recording_path);
      if (current) {
        newMap.set(recording_path, { ...current, phase });
      }
      return newMap;
    });
  });

  const openRecordingsFolder = async () => {
    try {
      await invoke("open_recordings_dir");
    } catch (err) {
      console.error("Failed to open recordings folder:", err);
    }
  };

  const deleteRecording = async (path: string) => {
    const confirmed = await ask("Are you sure you want to delete this recording?", {
      title: "Delete recording",
      kind: "warning",
      okLabel: "Delete",
      cancelLabel: "Cancel",
    });
    if (!confirmed) return;

    try {
      await invoke("delete_recording", { path });
      // Remove from transcription state
      setTranscriptionStates((prev) => {
        const newMap = new Map(prev);
        newMap.delete(path);
        return newMap;
      });
      await loadRecordings();
    } catch (err) {
      console.error("Failed to delete recording:", err);
      alert("Failed to delete recording. Please try again.");
    }
  };

  const checkTranscriptionResult = async (path: string): Promise<boolean> => {
    try {
      return await invoke<boolean>("has_transcription_result", { recordingPath: path });
    } catch {
      return false;
    }
  };

  const openTranscriptionResult = async (path: string) => {
    try {
      await invoke("open_transcription_window", { recordingPath: path });
    } catch (err) {
      console.error("Failed to open transcription result:", err);
      alert("Failed to open transcription result.");
    }
  };

  const transcribeRecording = async (path: string) => {
    try {
      await invoke("start_transcription", { recordingPath: path });
    } catch (err) {
      console.error("Failed to start transcription:", err);
      alert("Failed to start transcription. Please try again.");
    }
  };

  if (loading) {
    return (
      <div className="max-w-3xl w-full mx-auto space-y-6">
        <div className="space-y-2">
          <div className="px-4 flex items-center justify-between">
            <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
              Recordings
            </h2>
            <button
              type="button"
              onClick={openRecordingsFolder}
              className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>
        </div>
        <div className="flex flex-col items-center justify-center py-16 text-mid-gray">
          <Loader2 className="w-8 h-8 animate-spin mb-3" />
          <p className="text-sm">Loading recordings...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="max-w-3xl w-full mx-auto space-y-6">
        <div className="space-y-2">
          <div className="px-4 flex items-center justify-between">
            <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
              Recordings
            </h2>
            <button
              type="button"
              onClick={openRecordingsFolder}
              className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>
        </div>
        <div className="flex items-center justify-center py-16 text-red-500">
          <p>{error}</p>
        </div>
      </div>
    );
  }

  if (recordings.length === 0) {
    return (
      <div className="max-w-3xl w-full mx-auto space-y-6">
        <div className="space-y-2">
          <div className="px-4 flex items-center justify-between">
            <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
              Recordings
            </h2>
            <button
              type="button"
              onClick={openRecordingsFolder}
              className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>
        </div>
        <div className="flex flex-col items-center justify-center py-16 text-mid-gray">
          <FileText className="w-12 h-12 mb-3 opacity-50" />
          <p className="text-sm">No recordings yet</p>
          <p className="text-xs mt-1">Start recording to see your audio files here</p>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        <div className="px-4 flex items-center justify-between">
          <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
            Recordings
          </h2>
          <button
            type="button"
            onClick={openRecordingsFolder}
            className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
          >
            <FolderOpen className="w-4 h-4" />
            Open Folder
          </button>
        </div>
      </div>

      <div className="space-y-2">
        {recordings.map((recording) => {
          const transcriptionState = transcriptionStates.get(recording.path);
          const audioUrl = `stream://localhost/${encodeURIComponent(recording.path)}`;

          return (
            <RecordingEntry
              key={recording.path}
              recording={recording}
              audioUrl={audioUrl}
              transcriptionState={transcriptionState}
              isPlaying={currentlyPlayingPath === recording.path}
              onPlayStateChange={(playing) =>
                setCurrentlyPlayingPath(playing ? recording.path : null)
              }
              onDelete={() => deleteRecording(recording.path)}
              onTranscribe={() => transcribeRecording(recording.path)}
              onOpenResult={() => openTranscriptionResult(recording.path)}
              onCheckResult={checkTranscriptionResult}
              onRename={loadRecordings}
            />
          );
        })}
      </div>
    </div>
  );
};

interface RecordingEntryProps {
  recording: RecordingFile;
  audioUrl: string;
  transcriptionState?: TranscriptionState;
  isPlaying: boolean;
  onPlayStateChange: (playing: boolean) => void;
  onDelete: () => void;
  onTranscribe: () => void;
  onOpenResult: () => void;
  onCheckResult: (path: string) => Promise<boolean>;
  onRename: () => void;
}

const RecordingEntry: React.FC<RecordingEntryProps> = ({
  recording,
  audioUrl,
  transcriptionState,
  isPlaying,
  onPlayStateChange,
  onDelete,
  onTranscribe,
  onOpenResult,
  onCheckResult,
  onRename,
}) => {
  const [hasResult, setHasResult] = useState(false);
  const [isEditingName, setIsEditingName] = useState(false);
  const [editName, setEditName] = useState("");
  const [renameError, setRenameError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    onCheckResult(recording.path).then(setHasResult);
  }, [recording.path, onCheckResult]);

  // Update hasResult from transcription state
  useEffect(() => {
    if (transcriptionState?.hasResult) {
      setHasResult(true);
    }
  }, [transcriptionState?.hasResult]);

  // Focus and select all when editing starts
  useEffect(() => {
    if (isEditingName && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditingName]);

  const status = transcriptionState?.status || "idle";
  const progress = transcriptionState?.progress || 0;
  const etaSeconds = transcriptionState?.etaSeconds || null;
  const phase = transcriptionState?.phase || null;
  const transcriptionError = transcriptionState?.error || null;

  const handleNameClick = () => {
    // Extract name without extension
    const nameWithoutExt = recording.name.replace(/\.wav$/i, "");
    setEditName(nameWithoutExt);
    setRenameError(null);
    setIsEditingName(true);
  };

  const handleRename = async () => {
    const trimmed = editName.trim();
    if (!trimmed) {
      setIsEditingName(false);
      setRenameError(null);
      return;
    }
    
    const currentNameWithoutExt = recording.name.replace(/\.wav$/i, "");
    if (trimmed === currentNameWithoutExt) {
      setIsEditingName(false);
      setRenameError(null);
      return;
    }

    setRenameError(null);
    try {
      await invoke("rename_recording", { path: recording.path, newName: trimmed });
      setIsEditingName(false);
      onRename(); // Reload recordings list
    } catch (err) {
      setRenameError(err instanceof Error ? err.message : "Rename failed");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      handleRename();
    } else if (e.key === "Escape") {
      setIsEditingName(false);
      setRenameError(null);
    }
  };

  return (
    <div className="bg-background border border-mid-gray/10 rounded-lg p-4 hover:border-mid-gray/20 transition-colors">
      <div className="space-y-3">
        {/* Header */}
        <div className="flex items-start justify-between gap-3">
          <div className="flex-1 min-w-0">
            {isEditingName ? (
              <div>
                <input
                  ref={inputRef}
                  type="text"
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  onBlur={handleRename}
                  onKeyDown={handleKeyDown}
                  className="w-full px-2 py-1 text-sm font-medium bg-background border border-blue-500 rounded focus:outline-none focus:ring-2 focus:ring-blue-500/50"
                />
                {renameError && (
                  <p className="mt-1 text-xs text-red-500">{renameError}</p>
                )}
              </div>
            ) : (
              <button
                type="button"
                onClick={handleNameClick}
                className="text-left w-full group"
              >
                <h3 className="text-sm font-medium text-foreground truncate group-hover:text-blue-500 transition-colors">
                  {recording.name}
                </h3>
              </button>
            )}
            <div className="flex items-center gap-3 mt-1 text-xs text-mid-gray">
              <span>{formatDate(recording.created)}</span>
              <span>{formatFileSize(recording.size)}</span>
            </div>
          </div>
          <button
            type="button"
            onClick={onDelete}
            className="p-1.5 rounded hover:bg-red-500/10 hover:text-red-500 transition-colors"
            aria-label="Delete recording"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        </div>

        {/* Audio Player */}
        <AudioPlayer
          src={audioUrl}
          isActive={isPlaying}
          onPlayStateChange={onPlayStateChange}
          initialDuration={recording.duration_seconds}
          className="w-full"
        />

        {/* Transcription Status */}
        {status === "transcribing" && (
          <div className="space-y-2 p-3 bg-blue-500/5 border border-blue-500/20 rounded-md">
            <div className="flex items-center justify-between text-xs">
              <span className="text-blue-600 dark:text-blue-400 font-medium">
                {formatEta(etaSeconds, phase)}
              </span>
              <span className="text-mid-gray">{Math.round(progress * 100)}%</span>
            </div>
            <div className="w-full h-1.5 bg-mid-gray/10 rounded-full overflow-hidden">
              <div
                className="h-full bg-blue-500 transition-all duration-300"
                style={{ width: `${progress * 100}%` }}
              />
            </div>
          </div>
        )}

        {transcriptionError && (
          <div className="p-3 bg-red-500/5 border border-red-500/20 rounded-md">
            <p className="text-xs text-red-600 dark:text-red-400">{transcriptionError}</p>
          </div>
        )}

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onTranscribe}
            disabled={status === "transcribing"}
            className="flex-1 px-3 py-2 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {status === "transcribing" ? (
              <span className="flex items-center justify-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin" />
                Transcribing...
              </span>
            ) : (
              "Transcribe"
            )}
          </button>
          {hasResult && (
            <button
              type="button"
              onClick={onOpenResult}
              className="flex items-center gap-2 px-3 py-2 text-sm rounded-md bg-blue-500 text-white hover:bg-blue-600 transition-colors"
            >
              <ExternalLink className="w-4 h-4" />
              View Result
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
