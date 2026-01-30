import React, { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { FolderOpen, Trash2, Play, Pause } from "lucide-react";

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
    if (!confirm("Are you sure you want to delete this recording?")) {
      return;
    }

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
              onDelete={() => deleteRecording(recording.path)}
            />
          ))}
        </div>
      </div>
    </div>
  );
};

interface RecordingEntryProps {
  recording: RecordingFile;
  onDelete: () => void;
}

const RecordingEntry: React.FC<RecordingEntryProps> = ({
  recording,
  onDelete,
}) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [audioUrl, setAudioUrl] = useState<string>("");
  const audioRef = React.useRef<HTMLAudioElement>(null);

  useEffect(() => {
    const url = convertFileSrc(recording.path);
    setAudioUrl(url);
  }, [recording.path]);

  const togglePlay = () => {
    if (!audioRef.current) return;

    if (isPlaying) {
      audioRef.current.pause();
    } else {
      audioRef.current.play();
    }
    setIsPlaying(!isPlaying);
  };

  const handleAudioEnd = () => {
    setIsPlaying(false);
  };

  return (
    <div className="px-4 py-3 flex items-center justify-between">
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <button
            type="button"
            onClick={togglePlay}
            className="p-1 rounded hover:bg-mid-gray/10 transition-colors"
            title={isPlaying ? "Pause" : "Play"}
          >
            {isPlaying ? (
              <Pause size={18} className="text-logo-primary" />
            ) : (
              <Play size={18} className="text-mid-gray" />
            )}
          </button>
          <p className="text-sm font-medium truncate">{recording.name}</p>
        </div>
        <div className="flex items-center gap-3 text-xs text-mid-gray ml-8">
          <span>{formatDate(recording.created)}</span>
          <span>•</span>
          <span>{formatFileSize(recording.size)}</span>
        </div>
        <audio
          ref={audioRef}
          src={audioUrl}
          onEnded={handleAudioEnd}
          onPause={() => setIsPlaying(false)}
          onPlay={() => setIsPlaying(true)}
        />
      </div>
      <button
        type="button"
        onClick={onDelete}
        className="p-2 rounded hover:bg-red-500/10 text-mid-gray hover:text-red-500 transition-colors"
        title="Delete recording"
      >
        <Trash2 size={16} />
      </button>
    </div>
  );
};
