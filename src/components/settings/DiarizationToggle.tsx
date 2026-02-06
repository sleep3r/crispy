import React, { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useTauriListen } from "../../hooks/useTauriListen";
import { Download, Loader2, CheckCircle2 } from "lucide-react";

interface ModelInfo {
  id: string;
  name: string;
  is_downloaded: boolean;
  is_downloading: boolean;
  size_mb: number;
}

interface DownloadProgress {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

interface DiarizationToggleProps {
  grouped?: boolean;
}

const DIARIZATION_MODELS = ["diarize-segmentation", "diarize-embedding"];

export const DiarizationToggle: React.FC<DiarizationToggleProps> = ({
  grouped = false,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const enabled = getSetting("diarization_enabled") === "true";

  const [modelsReady, setModelsReady] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [modelStatuses, setModelStatuses] = useState<Record<string, boolean>>({});

  const checkModels = useCallback(async () => {
    try {
      const statuses: Record<string, boolean> = {};
      let allReady = true;
      for (const modelId of DIARIZATION_MODELS) {
        const info = await invoke<ModelInfo | null>("get_model_info", { modelId });
        statuses[modelId] = info?.is_downloaded ?? false;
        if (!info?.is_downloaded) allReady = false;
      }
      setModelStatuses(statuses);
      setModelsReady(allReady);
    } catch {
      setModelsReady(false);
    }
  }, []);

  useEffect(() => {
    checkModels();
  }, [checkModels]);

  // Listen for download progress and completion with proper lifecycle
  useTauriListen<DownloadProgress>("model-download-progress", (event) => {
    if (DIARIZATION_MODELS.includes(event.payload.model_id)) {
      setDownloadProgress((prev) => ({
        ...prev,
        [event.payload.model_id]: event.payload.percentage,
      }));
    }
  });

  useTauriListen<string>("model-download-complete", (event) => {
    if (DIARIZATION_MODELS.includes(event.payload)) {
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[event.payload];
        return next;
      });
      checkModels();
    }
  });

  const handleToggle = async () => {
    if (!enabled && !modelsReady) {
      // Need to download models first
      await downloadModels();
      return;
    }
    updateSetting("diarization_enabled", enabled ? "false" : "true");
  };

  const downloadModels = async () => {
    setDownloading(true);
    try {
      for (const modelId of DIARIZATION_MODELS) {
        if (!modelStatuses[modelId]) {
          await invoke("download_model", { modelId });
        }
      }
      await checkModels();
      // Enable after download
      updateSetting("diarization_enabled", "true");
    } catch (err) {
      console.error("Failed to download diarization models:", err);
    } finally {
      setDownloading(false);
    }
  };

  const totalProgress =
    Object.values(downloadProgress).length > 0
      ? Object.values(downloadProgress).reduce((a, b) => a + b, 0) /
        DIARIZATION_MODELS.length
      : 0;

  return (
    <SettingContainer
      title="Speaker Diarization"
      description={
        modelsReady
          ? "Identify different speakers in transcriptions."
          : "Identify different speakers in transcriptions. Requires downloading diarization models (~34 MB)."
      }
      grouped={grouped}
      layout="horizontal"
    >
      <div className="flex items-center gap-3">
        {downloading && (
          <div className="flex items-center gap-2 text-xs text-mid-gray">
            <Loader2 size={14} className="animate-spin" />
            <span>{Math.round(totalProgress)}%</span>
          </div>
        )}

        {!modelsReady && !downloading && (
          <button
            type="button"
            onClick={downloadModels}
            className="flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-md bg-mid-gray/10 hover:bg-mid-gray/20 text-mid-gray hover:text-text transition-colors"
            title="Download diarization models"
          >
            <Download size={13} />
            <span>Download</span>
          </button>
        )}

        {modelsReady && !enabled && (
          <CheckCircle2 size={14} className="text-green-500 shrink-0" />
        )}

        <label
          className="relative inline-flex items-center cursor-pointer"
          aria-label="Toggle diarization"
        >
          <input
            type="checkbox"
            checked={enabled}
            onChange={handleToggle}
            disabled={downloading}
            className="sr-only peer"
            aria-label="Speaker diarization"
          />
          <div className="w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-logo-primary/50 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-mid-gray/20 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-logo-primary peer-disabled:opacity-50" />
        </label>
      </div>
    </SettingContainer>
  );
};
