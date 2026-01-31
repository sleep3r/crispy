import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Mic, Square, ExternalLink, Power } from "lucide-react";

export const TrayPopupView: React.FC = () => {
  const [recording, setRecording] = useState(false);
  const [loading, setLoading] = useState(true);

  const refreshRecording = async () => {
    try {
      const active = await invoke<boolean>("is_recording");
      setRecording(active);
    } catch {
      setRecording(false);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refreshRecording();
  }, []);

  const handleStart = async () => {
    try {
      await invoke("start_recording", { appId: "" });
      setRecording(true);
    } catch {
      setRecording(false);
    }
  };

  const handleStop = async () => {
    try {
      await invoke("stop_recording");
      setRecording(false);
    } catch {
      await refreshRecording();
    }
  };

  const handleOpen = async () => {
    await invoke("show_main_window_cmd");
    getCurrentWindow().hide();
  };

  const handleQuit = async () => {
    await invoke("quit_app");
  };

  return (
    <div className="h-screen overflow-hidden bg-background text-text flex flex-col pt-3 px-3 pb-0 rounded-t-lg border-x border-t border-mid-gray/20 shadow-lg">
      <div className="shrink-0 flex items-center justify-between mb-2 pb-2 border-b border-mid-gray/20">
        <span className="text-sm font-semibold">Crispy</span>
      </div>
      <div className="shrink-0 flex flex-col gap-1.5">
        <button
          type="button"
          onClick={handleStart}
          disabled={recording || loading}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm font-medium bg-mid-gray/10 hover:bg-mid-gray/20 disabled:opacity-50 disabled:pointer-events-none transition-colors text-left"
        >
          <Mic size={16} />
          Start recording
        </button>
        <button
          type="button"
          onClick={handleStop}
          disabled={!recording || loading}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm font-medium bg-mid-gray/10 hover:bg-mid-gray/20 disabled:opacity-50 disabled:pointer-events-none transition-colors text-left"
        >
          <Square size={16} />
          Stop recording
        </button>
        <div className="my-1 border-t border-mid-gray/20" />
        <button
          type="button"
          onClick={handleOpen}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm font-medium bg-mid-gray/10 hover:bg-mid-gray/20 transition-colors text-left"
        >
          <ExternalLink size={16} />
          Open app
        </button>
        <button
          type="button"
          onClick={handleQuit}
          className="flex items-center gap-2 w-full px-3 py-2 rounded-md text-sm font-medium bg-mid-gray/10 hover:bg-red-500/20 hover:text-red-600 dark:hover:text-red-400 transition-colors text-left"
        >
          <Power size={16} />
          Quit
        </button>
      </div>
    </div>
  );
};
