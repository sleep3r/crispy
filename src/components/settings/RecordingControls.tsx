import React, { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { Circle, Square } from "lucide-react";

export const RecordingControls: React.FC = () => {
  const [isRecording, setIsRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const checkRecordingStatus = useCallback(async () => {
    try {
      const recording = await invoke<boolean>("is_recording");
      setIsRecording(recording);
    } catch (err) {
      console.error("Failed to check recording status:", err);
    }
  }, []);

  useEffect(() => {
    checkRecordingStatus();
    
    const interval = setInterval(checkRecordingStatus, 1000);
    return () => clearInterval(interval);
  }, [checkRecordingStatus]);

  const handleStartRecording = async () => {
    try {
      setError(null);
      await invoke("start_recording", { appId: "system" });
      setIsRecording(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to start recording");
    }
  };

  const handleStopRecording = async () => {
    try {
      setError(null);
      const outputPath = await invoke<string>("stop_recording");
      setIsRecording(false);
      console.log("Recording saved to:", outputPath);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to stop recording");
    }
  };

  return (
    <SettingContainer
      title="Recording"
      description="Record processed mic + app audio to MP3."
      grouped
      layout="stacked"
    >
      <div className="flex flex-col gap-4">
        {error && (
          <div className="px-3 py-2 rounded-md border border-red-500/30 bg-red-500/10 text-xs text-red-700">
            {error}
          </div>
        )}

        <div className="flex items-center gap-2">
          {!isRecording ? (
            <button
              type="button"
              onClick={handleStartRecording}
              className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md bg-red-500 text-white hover:bg-red-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              <Circle size={16} className="fill-current" />
              Start Recording
            </button>
          ) : (
            <button
              type="button"
              onClick={handleStopRecording}
              className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-md bg-mid-gray/20 text-text hover:bg-mid-gray/30 transition-colors"
            >
              <Square size={16} className="fill-current" />
              Stop Recording
            </button>
          )}
          {isRecording && (
            <span className="text-xs text-mid-gray animate-pulse">Recording...</span>
          )}
        </div>

        <div className="flex flex-col gap-1 text-xs text-mid-gray">
          <div>Recordings are saved to: ~/Documents/Crispy/Recordings/</div>
          <div className="text-yellow-600">
            Note: Records to WAV format. Currently mic only, app audio coming soon.
          </div>
        </div>
      </div>
    </SettingContainer>
  );
};
