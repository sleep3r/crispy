import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { PathDisplay } from "../ui/PathDisplay";

interface RecordingsDirectoryProps {
  grouped?: boolean;
}

export const RecordingsDirectory: React.FC<RecordingsDirectoryProps> = ({
  grouped = false,
}) => {
  const [dirPath, setDirPath] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadRecordingsDirectory = async () => {
      try {
        const result = await invoke<string>("get_recordings_dir_path");
        setDirPath(result);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to load recordings directory",
        );
      } finally {
        setLoading(false);
      }
    };

    loadRecordingsDirectory();
  }, []);

  const handleOpen = async () => {
    if (!dirPath) return;
    try {
      await invoke("open_recordings_dir");
    } catch (openError) {
      console.error("Failed to open recordings directory:", openError);
    }
  };

  if (loading) {
    return (
      <SettingContainer
        title="Recordings"
        description="Location where recordings are saved."
        grouped={grouped}
        layout="stacked"
      >
        <div className="animate-pulse">
          <div className="h-8 bg-mid-gray/10 rounded"></div>
        </div>
      </SettingContainer>
    );
  }

  if (error) {
    return (
      <SettingContainer
        title="Recordings"
        description="Location where recordings are saved."
        grouped={grouped}
        layout="stacked"
      >
        <div className="px-3 py-2 rounded-md border border-red-500/30 bg-red-500/10 text-xs text-red-700">
          {error}
        </div>
      </SettingContainer>
    );
  }

  return (
    <SettingContainer
      title="Recordings"
      description="Location where recordings are saved."
      grouped={grouped}
      layout="stacked"
    >
      <PathDisplay
        path={dirPath}
        onOpen={handleOpen}
        disabled={!dirPath}
      />
    </SettingContainer>
  );
};
