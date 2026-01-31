import React, { useEffect, useRef, useState } from "react";
import { useTranscriptionModels } from "../../hooks/useTranscriptionModels";
import { formatModelSize } from "../../lib/utils/format";
import { SettingContainer } from "../ui/SettingContainer";

const getStatusMeta = (
  status: string,
  error: string | null,
  currentModelId: string
) => {
  if (error) {
    return { text: `Error: ${error}`, className: "text-red-500", dot: "bg-red-500" };
  }
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
      return {
        text: `Active model: ${currentModelId}`,
        className: "text-green-500",
        dot: "bg-green-500",
      };
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

export const TranscriptionModelSelector: React.FC<{ grouped?: boolean }> = ({
  grouped = false,
}) => {
  const {
    models,
    loading,
    error,
    selected,
    current,
    setActiveModel,
    downloadModel,
    downloadProgress,
    currentModelId,
    modelStatus,
    modelError,
    extractingModels,
    downloadStats,
    cancelDownload,
  } = useTranscriptionModels();
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
    await setActiveModel(value);
    setIsOpen(false);
  };

  const handleDownloadClick = async (
    e: React.MouseEvent,
    modelId: string
  ) => {
    e.stopPropagation();
    if (modelId in downloadProgress) return;
    try {
      await downloadModel(modelId);
    } catch (err) {
      console.error(err);
    }
  };

  const handleCancelClick = async (
    e: React.MouseEvent,
    modelId: string
  ) => {
    e.stopPropagation();
    try {
      await cancelDownload(modelId);
    } catch (err) {
      console.error(err);
    }
  };

  const downloadableModels = models.filter(
    (m) => m.id !== "none" && !m.is_downloaded
  );
  const availableModels = models.filter(
    (m) => m.id !== "none" && m.is_downloaded
  );
  const statusMeta = getStatusMeta(modelStatus, modelError, currentModelId);

  const content = (
    <div className="space-y-4">
      <div className="relative" ref={dropdownRef}>
        <button
          type="button"
          onClick={() => setIsOpen(!isOpen)}
          disabled={loading}
          className="flex items-center gap-2 px-3 py-1.5 w-full rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors text-left disabled:opacity-60"
        >
          <span className="text-sm flex-1 truncate">
            {loading ? "Loading…" : current.name}
          </span>
          <svg
            className={`w-4 h-4 shrink-0 text-mid-gray transition-transform ${
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

        {error && (
          <p className="text-xs text-red-500 mt-1">{error}</p>
        )}

        {isOpen && (
          <div className="absolute top-full left-0 right-0 mt-1 bg-background border border-mid-gray/20 rounded-lg shadow-lg py-1 z-50 max-h-64 overflow-y-auto">
            {models.map((model) => (
              <button
                key={model.id}
                type="button"
                onClick={() => handleSelect(model.id)}
                disabled={model.id !== "none" && !model.is_downloaded}
                className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors disabled:opacity-60 disabled:cursor-not-allowed ${
                  selected === model.id ? "bg-mid-gray/10" : ""
                }`}
              >
                <div className="text-sm font-medium">{model.name}</div>
                <div className="text-xs text-mid-gray">
                  {model.id === "none"
                    ? model.description
                    : [
                        model.description,
                        model.size_mb > 0 && formatModelSize(model.size_mb),
                        model.is_downloaded && "Downloaded",
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      {downloadableModels.length > 0 && (
        <div>
          <div className="text-xs font-medium text-mid-gray mb-2">
            Download models
          </div>
          {availableModels.length === 0 && (
            <div className="text-xs text-mid-gray mb-2">
              No model – Download required
            </div>
          )}
          <ul className="space-y-2">
            {downloadableModels.map((model) => {
              const progress = downloadProgress[model.id];
              const isDownloading = Boolean(progress);
              const isExtracting = Boolean(extractingModels[model.id]);
              const stats = downloadStats[model.id];
              return (
                <li
                  key={model.id}
                  className="flex items-start justify-between gap-3 px-3 py-2 rounded-md border border-mid-gray/20 bg-mid-gray/5"
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium">{model.name}</div>
                    <div className="text-xs text-mid-gray">
                      {model.description}
                    </div>
                    <div className="text-xs text-mid-gray mt-0.5 tabular-nums">
                      {[
                        formatModelSize(model.size_mb),
                        progress && stats ? `${stats.speed.toFixed(1)}MB/s` : null,
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                    </div>
                    {isDownloading && progress && (
                      <div className="mt-2 h-1.5 w-full rounded-full bg-mid-gray/20 overflow-hidden">
                        <div
                          className="h-full bg-logo-primary rounded-full transition-all duration-300"
                          style={{
                            width: `${Math.max(
                              0,
                              Math.min(100, progress.percentage)
                            )}%`,
                          }}
                        />
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={(e) => handleDownloadClick(e, model.id)}
                    disabled={isDownloading}
                    className="shrink-0 text-xs font-medium text-logo-primary hover:underline disabled:opacity-60 disabled:no-underline"
                  >
                    {getDownloadLabel(progress, isExtracting)}
                  </button>
                  {isDownloading && (
                    <button
                      type="button"
                      onClick={(e) => handleCancelClick(e, model.id)}
                      className="shrink-0 text-[11px] text-red-400 hover:text-red-300 px-1 py-0.5 rounded hover:bg-red-500/10 transition-colors"
                    >
                      Cancel
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {(modelStatus !== "none" || modelError) && (
        <div className={`text-xs flex items-center gap-2 ${statusMeta.className}`}>
          <span className={`inline-block w-1.5 h-1.5 rounded-full ${statusMeta.dot}`} />
          {statusMeta.text}
        </div>
      )}
    </div>
  );

  if (grouped) {
    return (
      <SettingContainer
        title="Transcription model"
        description="Model used to transcribe recordings."
        grouped
      >
        {content}
      </SettingContainer>
    );
  }

  return (
    <SettingContainer
      title="Transcription model"
      description="Model used to transcribe recordings."
    >
      {content}
    </SettingContainer>
  );
};
