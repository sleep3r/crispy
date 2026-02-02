import React, { useState, useEffect, useCallback, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Trash2, FileText, Loader2, ExternalLink } from "lucide-react";
import { AudioPlayer } from "../../ui/AudioPlayer";

interface RecordingFile {
  name: string;
  path: string;
  size: number;
  created: number;
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

export const RecordingsHistory: React.FC = () => {
  const [recordings, setRecordings] = useState<RecordingFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentlyPlayingPath, setCurrentlyPlayingPath] = useState<string | null>(null);

  const loadRecordings = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await invoke<RecordingFile[]>("get_recordings");
      setRecordings(result);
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
      await loadRecordings();
    } catch (err) {
      console.error("Failed to delete recording:", err);
      alert("Failed to delete recording. Please try again.");
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
              <FolderOpen size={16} />
              <span>Open Folder</span>
            </button>
          </div>
          <div className="bg-background border border-mid-gray/20 rounded-lg">
            <div className="px-4 py-3 text-center text-mid-gray">
              Loading recordings...
            </div>
          </div>
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
              <FolderOpen size={16} />
              <span>Open Folder</span>
            </button>
          </div>
          <div className="bg-background border border-mid-gray/20 rounded-lg">
            <div className="px-4 py-3 text-center text-red-600 text-sm">
              {error}
            </div>
          </div>
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
              <FolderOpen size={16} />
              <span>Open Folder</span>
            </button>
          </div>
          <div className="bg-background border border-mid-gray/20 rounded-lg">
            <div className="px-4 py-3 text-center text-mid-gray">
              No recordings yet. Start recording from the General tab.
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        <div className="px-4 flex items-center justify-between">
          <h2 className="text-xs font-medium text-mid-gray uppercase tracking-wide">
            Recordings ({recordings.length})
          </h2>
          <button
            type="button"
            onClick={openRecordingsFolder}
            className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
          >
            <FolderOpen size={16} />
            <span>Open Folder</span>
          </button>
        </div>
        <div className="bg-background border border-mid-gray/20 rounded-lg divide-y divide-mid-gray/20">
          {recordings.map((recording) => (
            <RecordingEntry
              key={recording.path}
              recording={recording}
              isActive={currentlyPlayingPath === recording.path}
              onPlayStateChange={(playing) => {
                setCurrentlyPlayingPath(playing ? recording.path : null);
              }}
              onDelete={() => deleteRecording(recording.path)}
              onRename={loadRecordings}
            />
          ))}
        </div>
      </div>
    </div>
  );
};

type TranscriptionStatus = "idle" | "transcribing" | "completed" | "error";

function nameWithoutExtension(name: string): string {
  return name.replace(/\.wav$/i, "");
}

interface RecordingEntryProps {
  recording: RecordingFile;
  isActive: boolean;
  onPlayStateChange: (playing: boolean) => void;
  onDelete: () => void;
  onRename: () => void;
}

interface TranscriptionStatusEvent {
  recording_path: string;
  status: string;
  error?: string | null;
}

const RecordingEntry: React.FC<RecordingEntryProps> = ({
  recording,
  isActive,
  onPlayStateChange,
  onDelete,
  onRename,
}) => {
  const [audioUrl, setAudioUrl] = useState<string>("");
  const [transcriptionStatus, setTranscriptionStatus] = useState<TranscriptionStatus>("idle");
  const [transcriptionError, setTranscriptionError] = useState<string | null>(null);
  const [hasResult, setHasResult] = useState(false);
  const [isEditingName, setIsEditingName] = useState(false);
  const [editName, setEditName] = useState(nameWithoutExtension(recording.name));
  const [renameError, setRenameError] = useState<string | null>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setEditName(nameWithoutExtension(recording.name));
  }, [recording.name]);

  useEffect(() => {
    if (isEditingName && nameInputRef.current) {
      nameInputRef.current.focus();
      nameInputRef.current.select();
    }
  }, [isEditingName]);

  useEffect(() => {
    let cancelled = false;
    invoke<boolean>("has_transcription_result", { recordingPath: recording.path })
      .then((ok) => {
        if (!cancelled) setHasResult(ok);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [recording.path, transcriptionStatus]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<TranscriptionStatusEvent>("transcription-status", (event) => {
      if (event.payload.recording_path !== recording.path) return;
      const s = event.payload.status;
      if (s === "started") {
        setTranscriptionStatus("transcribing");
        setTranscriptionError(null);
      } else if (s === "completed") {
        setTranscriptionStatus("completed");
        setHasResult(true);
        setTranscriptionError(null);
      } else if (s === "error") {
        setTranscriptionStatus("error");
        setTranscriptionError(event.payload.error ?? "Transcription failed");
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [recording.path]);

  useEffect(() => {
    setAudioUrl(convertFileSrc(recording.path));
  }, [recording.path]);

  const startTranscription = async () => {
    setTranscriptionStatus("transcribing");
    setTranscriptionError(null);
    try {
      await invoke("start_transcription", { recordingPath: recording.path });
    } catch (err) {
      setTranscriptionStatus("error");
      setTranscriptionError(err instanceof Error ? err.message : "Failed to start");
    }
  };

  const openResultWindow = async () => {
    try {
      await invoke("open_transcription_window", { recordingPath: recording.path });
    } catch (err) {
      console.error("Failed to open transcription window:", err);
    }
  };

  const saveRename = async () => {
    const trimmed = editName.trim();
    if (!trimmed || trimmed === nameWithoutExtension(recording.name)) {
      setIsEditingName(false);
      setRenameError(null);
      return;
    }
    setRenameError(null);
    try {
      await invoke("rename_recording", { path: recording.path, newName: trimmed });
      setIsEditingName(false);
      onRename();
    } catch (err) {
      setRenameError(err instanceof Error ? err.message : "Rename failed");
    }
  };

  const cancelRename = () => {
    setEditName(nameWithoutExtension(recording.name));
    setRenameError(null);
    setIsEditingName(false);
  };

  return (
    <div className="px-4 py-3 flex flex-col gap-2">
      <div className="flex items-center justify-between">
        {isEditingName ? (
          <input
            ref={nameInputRef}
            type="text"
            value={editName}
            onChange={(e) => setEditName(e.target.value)}
            onBlur={saveRename}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                (e.target as HTMLInputElement).blur();
              } else if (e.key === "Escape") {
                cancelRename();
                nameInputRef.current?.blur();
              }
            }}
            className="text-sm font-medium bg-mid-gray/10 border border-mid-gray/30 rounded px-2 py-1 min-w-0 flex-1"
            aria-label="Recording name"
          />
        ) : (
          <button
            type="button"
            onClick={() => setIsEditingName(true)}
            className="text-sm font-medium truncate text-left hover:underline focus:outline-none focus:ring-1 focus:ring-mid-gray/30 rounded"
            title="Click to rename"
          >
            {recording.name}
          </button>
        )}
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={startTranscription}
            disabled={transcriptionStatus === "transcribing"}
            className="p-2 rounded hover:bg-mid-gray/10 text-mid-gray hover:text-text transition-colors disabled:opacity-50"
            title="Transcribe"
          >
            {transcriptionStatus === "transcribing" ? (
              <Loader2 size={16} className="animate-spin" />
            ) : (
              <FileText size={16} />
            )}
          </button>
          {(hasResult || transcriptionStatus === "completed") && (
            <button
              type="button"
              onClick={openResultWindow}
              className="p-2 rounded hover:bg-mid-gray/10 text-mid-gray hover:text-text transition-colors"
              title="View transcription result"
            >
              <ExternalLink size={16} />
            </button>
          )}
          <button
            type="button"
            onClick={onDelete}
            className="p-2 rounded hover:bg-red-500/10 text-mid-gray hover:text-red-500 transition-colors"
            title="Delete recording"
          >
            <Trash2 size={16} />
          </button>
        </div>
      </div>
      {transcriptionStatus === "error" && transcriptionError && (
        <p className="text-xs text-red-500">{transcriptionError}</p>
      )}
      {renameError && (
        <p className="text-xs text-red-500">{renameError}</p>
      )}
      <div className="flex items-center gap-3 text-xs text-mid-gray">
        <span>{formatDate(recording.created)}</span>
        <span>•</span>
        <span>{formatFileSize(recording.size)}</span>
      </div>
      <AudioPlayer
        src={audioUrl}
        isActive={isActive}
        onPlayStateChange={onPlayStateChange}
        className="w-full"
      />
    </div>
  );
};
