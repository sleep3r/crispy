import React, { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useTauriListen } from "../../hooks/useTauriListen";
import { Download, Loader2, CheckCircle2, ChevronDown, ChevronUp, RotateCcw } from "lucide-react";

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

const DEFAULTS = {
  max_speakers: "3",
  threshold: "0.50",
  merge_gap: "2.5",
};

export const DiarizationToggle: React.FC<DiarizationToggleProps> = ({
  grouped = false,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const enabled = getSetting("diarization_enabled") === "true";

  const [modelsReady, setModelsReady] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});
  const [modelStatuses, setModelStatuses] = useState<Record<string, boolean>>({});
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Diarization hyperparameters
  const maxSpeakers = Number.parseInt(getSetting("diarization_max_speakers") || DEFAULTS.max_speakers, 10);
  const threshold = Number.parseFloat(getSetting("diarization_threshold") || DEFAULTS.threshold);
  const mergeGap = Number.parseFloat(getSetting("diarization_merge_gap") || DEFAULTS.merge_gap);

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
      updateSetting("diarization_enabled", "true");
    } catch (err) {
      console.error("Failed to download diarization models:", err);
    } finally {
      setDownloading(false);
    }
  };

  const resetToDefaults = () => {
    updateSetting("diarization_max_speakers", DEFAULTS.max_speakers);
    updateSetting("diarization_threshold", DEFAULTS.threshold);
    updateSetting("diarization_merge_gap", DEFAULTS.merge_gap);
  };

  const isDefault =
    maxSpeakers === Number.parseInt(DEFAULTS.max_speakers, 10) &&
    threshold === Number.parseFloat(DEFAULTS.threshold) &&
    mergeGap === Number.parseFloat(DEFAULTS.merge_gap);

  const totalProgress =
    Object.values(downloadProgress).length > 0
      ? Object.values(downloadProgress).reduce((a, b) => a + b, 0) /
        DIARIZATION_MODELS.length
      : 0;

  return (
    <div>
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

      {/* Advanced settings - only when enabled */}
      {enabled && (
        <div className="px-4 pb-3">
          <button
            type="button"
            onClick={() => setShowAdvanced(!showAdvanced)}
            className="flex items-center gap-1.5 text-[11px] text-mid-gray/50 hover:text-mid-gray/80 transition-colors"
          >
            {showAdvanced ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            <span>Advanced settings</span>
          </button>

          {showAdvanced && (
            <div className="mt-3 space-y-4 pl-1">
              {/* Max Speakers */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Max speakers</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums w-6 text-right">{maxSpeakers}</span>
                </div>
                <input
                  type="range"
                  min={2}
                  max={12}
                  step={1}
                  value={maxSpeakers}
                  onChange={(e) => updateSetting("diarization_max_speakers", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>2</span>
                  <span>12</span>
                </div>
              </div>

              {/* Similarity Threshold */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Sensitivity</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums">{threshold.toFixed(2)}</span>
                </div>
                <input
                  type="range"
                  min={0.1}
                  max={0.8}
                  step={0.05}
                  value={threshold}
                  onChange={(e) => updateSetting("diarization_threshold", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>Fewer speakers</span>
                  <span>More speakers</span>
                </div>
              </div>

              {/* Merge Gap */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-mid-gray/70">Merge gap</span>
                  <span className="text-xs text-mid-gray/50 tabular-nums">{mergeGap.toFixed(1)}s</span>
                </div>
                <input
                  type="range"
                  min={0.5}
                  max={5}
                  step={0.5}
                  value={mergeGap}
                  onChange={(e) => updateSetting("diarization_merge_gap", e.target.value)}
                  className="w-full h-1 bg-mid-gray/15 rounded-full appearance-none cursor-pointer accent-logo-primary [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-logo-primary [&::-webkit-slider-thumb]:appearance-none"
                />
                <div className="flex justify-between text-[10px] text-mid-gray/30 mt-0.5">
                  <span>0.5s</span>
                  <span>5.0s</span>
                </div>
              </div>

              {/* Reset to defaults */}
              {!isDefault && (
                <button
                  type="button"
                  onClick={resetToDefaults}
                  className="flex items-center gap-1.5 text-[11px] text-mid-gray/40 hover:text-mid-gray/70 transition-colors mt-1"
                >
                  <RotateCcw size={11} />
                  <span>Reset to defaults</span>
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
