import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "../../hooks/useSettings";
import { useTranscriptionModels } from "../../hooks/useTranscriptionModels";
import { formatModelSize } from "../../lib/utils/format";
import type { SidebarSection } from "../Sidebar";

interface NsModelInfo {
  id: string;
  name: string;
  description: string;
}

const getStatusMeta = (status: string, error: string | null) => {
  if (error) return { text: `Error: ${error}`, className: "text-red-500", dot: "bg-red-500" };
  switch (status) {
    case "loading":
      return { text: "Initializing model…", className: "text-yellow-500", dot: "bg-yellow-500" };
    case "downloading":
      return { text: "Downloading model…", className: "text-logo-primary", dot: "bg-logo-primary" };
    case "extracting":
      return { text: "Extracting model…", className: "text-purple-500", dot: "bg-purple-500" };
    case "unloaded":
      return { text: "No model loaded", className: "text-mid-gray", dot: "bg-mid-gray" };
    case "ready":
      return { text: "Model ready", className: "text-green-500", dot: "bg-green-500" };
    default:
      return { text: "", className: "text-mid-gray", dot: "bg-mid-gray" };
  }
};

const getDownloadLabel = (
  progress: { percentage: number } | undefined,
  isExtracting: boolean
) => {
  if (progress) return `${Math.round(progress.percentage)}%`;
  if (isExtracting) return "Extracting…";
  return "Download";
};

const DEFAULT_NOISE_MODELS: NsModelInfo[] = [
  { id: "dummy", name: "None", description: "No processing" },
  { id: "noisy", name: "Test noise", description: "Adds test noise (debug)" },
  { id: "rnnnoise", name: "RNN Noise", description: "RNNoise denoiser (48 kHz)" },
];

interface FooterModelSelectorProps {
  currentSection: SidebarSection;
}

export const FooterModelSelector: React.FC<FooterModelSelectorProps> = ({
  currentSection,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const {
    models: transcriptionModels,
    selected: selectedTranscription,
    current: currentTranscription,
    setActiveModel: setTranscriptionModel,
    loading: transcriptionLoading,
    currentModelId,
    modelStatus,
    modelError,
    extractingModels,
    downloadModel,
    deleteModel,
    downloadProgress,
    downloadStats,
    cancelDownload,
  } = useTranscriptionModels();
  const isNoise = currentSection === "general";
  const [noiseModels, setNoiseModels] = useState<NsModelInfo[]>(DEFAULT_NOISE_MODELS);

  useEffect(() => {
    if (isNoise) {
      invoke<NsModelInfo[]>("get_available_ns_models")
        .then(setNoiseModels)
        .catch(() => setNoiseModels(DEFAULT_NOISE_MODELS));
    }
  }, [isNoise]);

  const models = isNoise ? noiseModels : transcriptionModels;
  const settingKey = isNoise ? "selected_model" : "selected_transcription_model";
  const selected = isNoise
    ? getSetting(settingKey) || "dummy"
    : selectedTranscription;
  const current = isNoise
    ? models.find((m) => m.id === selected) ?? models[0]
    : currentTranscription;

  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const handleSelect = async (value: string) => {
    if (isNoise) {
      await updateSetting(settingKey, value);
    } else {
      await setTranscriptionModel(value);
    }
    setIsOpen(false);
  };

  const handleDownloadClick = async (
    event: React.MouseEvent,
    modelId: string
  ) => {
    event.preventDefault();
    event.stopPropagation();
    if (modelId in downloadProgress) return;
    try {
      await downloadModel(modelId);
    } catch (err) {
      console.error(err);
    }
  };

  const handleDeleteClick = async (
    event: React.MouseEvent,
    modelId: string
  ) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      await deleteModel(modelId);
    } catch (err) {
      console.error(err);
    }
  };

  const handleCancelClick = async (
    event: React.MouseEvent,
    modelId: string
  ) => {
    event.preventDefault();
    event.stopPropagation();
    try {
      await cancelDownload(modelId);
    } catch (err) {
      console.error(err);
    }
  };

  const statusColor =
    isNoise && selected === "noisy" ? "bg-yellow-400" : "bg-green-500";
  const statusMeta = getStatusMeta(modelStatus, modelError);

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 bg-mid-gray/5 rounded-md border border-mid-gray/10 hover:bg-mid-gray/10 transition-colors"
      >
        <div className={`w-2 h-2 rounded-full ${statusColor}`} />
        <span className="text-xs font-medium max-w-40 truncate">
          {!isNoise && transcriptionLoading ? "Loading…" : current.name}
        </span>
        <svg
          className={`w-3 h-3 transition-transform ${
            isOpen ? "rotate-180" : ""
          }`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute bottom-full left-0 mb-2 w-64 bg-background border border-mid-gray/20 rounded-lg shadow-lg py-1 z-50">
          <div className="px-3 py-1.5 text-xs font-medium text-mid-gray border-b border-mid-gray/20">
            {isNoise ? "Noise suppression" : "Transcription"}
          </div>
          {isNoise ? (
            models.map((model) => (
              <button
                key={model.id}
                type="button"
                onClick={() => handleSelect(model.id)}
                className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                  selected === model.id ? "bg-logo-primary/10" : ""
                }`}
              >
                <div className="text-sm font-medium">{model.name}</div>
                <div className="text-xs text-mid-gray">
                  {model.description}
                </div>
              </button>
            ))
          ) : (
            <>
              <div className="px-3 py-1 text-xs font-medium text-mid-gray">
                Available models
              </div>
              {transcriptionModels
                .filter((m) => m.id === "none")
                .map((model) => (
                  <button
                    key={model.id}
                    type="button"
                    onClick={() => handleSelect(model.id)}
                    className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                      selected === model.id ? "bg-logo-primary/10" : ""
                    }`}
                  >
                    <div className="text-sm font-medium">{model.name}</div>
                    <div className="text-xs text-mid-gray">
                      {model.description}
                    </div>
                  </button>
                ))}

              {transcriptionModels
                .filter((m) => m.id !== "none" && m.is_downloaded)
                .map((model) => (
                  <button
                    key={model.id}
                    type="button"
                    onClick={() => handleSelect(model.id)}
                    className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                      selected === model.id ? "bg-logo-primary/10" : ""
                    }`}
                  >
                    <div className="flex items-center justify-between gap-2">
                      <div className="min-w-0">
                        <div className="text-sm font-medium">
                          {model.name}
                        </div>
                        <div className="text-xs text-mid-gray">
                          {[model.description, formatModelSize(model.size_mb)]
                            .filter(Boolean)
                            .join(" · ")}
                        </div>
                      </div>
                      <div className="flex items-center gap-2">
                        {currentModelId === model.id && (
                          <span className="text-[10px] text-logo-primary">
                            Active
                          </span>
                        )}
                        {currentModelId !== model.id && (
                          <button
                            onClick={(e) => handleDeleteClick(e, model.id)}
                            className="text-red-400 hover:text-red-300 p-1 hover:bg-red-500/10 rounded transition-colors"
                            title={`Delete ${model.name}`}
                          >
                            <svg
                              className="w-3 h-3"
                              fill="currentColor"
                              viewBox="0 0 20 20"
                            >
                              <path
                                fillRule="evenodd"
                                d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z"
                                clipRule="evenodd"
                              />
                            </svg>
                          </button>
                        )}
                      </div>
                    </div>
                  </button>
                ))}

              {transcriptionModels.some(
                (m) => m.id !== "none" && !m.is_downloaded
              ) && (
                <div className="border-t border-mid-gray/20 my-1" />
              )}

              {transcriptionModels.some(
                (m) => m.id !== "none" && !m.is_downloaded
              ) && (
                <div className="px-3 py-1 text-xs font-medium text-mid-gray">
                  Download models
                </div>
              )}

              {transcriptionModels
                .filter((m) => m.id !== "none" && !m.is_downloaded)
                .map((model) => {
                  const progress = downloadProgress[model.id];
                  const isExtracting = Boolean(extractingModels[model.id]);
                  const stats = downloadStats[model.id];
                  return (
                    <button
                      key={model.id}
                      type="button"
                      onClick={(e) => handleDownloadClick(e, model.id)}
                      className="w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <div className="min-w-0">
                          <div className="text-sm font-medium">
                            {model.name}
                          </div>
                          <div className="text-xs text-mid-gray">
                            {[
                              model.description,
                              formatModelSize(model.size_mb),
                              progress && stats
                                ? `${stats.speed.toFixed(1)}MB/s`
                                : null,
                            ]
                              .filter(Boolean)
                              .join(" · ")}
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <div className="text-xs text-logo-primary tabular-nums">
                            {getDownloadLabel(progress, isExtracting)}
                          </div>
                          {progress && (
                            <button
                              onClick={(e) => handleCancelClick(e, model.id)}
                              className="text-[11px] text-red-400 hover:text-red-300 px-1 py-0.5 rounded hover:bg-red-500/10 transition-colors"
                            >
                              Cancel
                            </button>
                          )}
                        </div>
                      </div>
                    </button>
                  );
                })}
            </>
          )}
          {!isNoise && (modelStatus !== "none" || modelError) && (
              <div className={`px-3 py-1 text-[11px] border-t border-mid-gray/10 flex items-center gap-2 ${statusMeta.className}`}>
                <span className={`inline-block w-1.5 h-1.5 rounded-full ${statusMeta.dot}`} />
                {statusMeta.text}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
