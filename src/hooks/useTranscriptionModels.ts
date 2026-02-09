import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettings } from "./useSettings";
import { useTauriListen } from "./useTauriListen";

export interface TranscriptionModelInfo {
  id: string;
  name: string;
  description: string;
  size_mb: number;
  is_downloaded: boolean;
  is_downloading: boolean;
}

export interface DownloadProgressPayload {
  model_id: string;
  downloaded: number;
  total: number;
  percentage: number;
}

interface ModelStateEvent {
  event_type: string;
  model_id?: string;
  model_name?: string;
  error?: string;
}

type ModelStatus =
  | "ready"
  | "loading"
  | "downloading"
  | "extracting"
  | "error"
  | "unloaded"
  | "none";

interface DownloadStats {
  startTime: number;
  lastUpdate: number;
  totalDownloaded: number;
  speed: number;
}

const NONE_OPTION: TranscriptionModelInfo = {
  id: "none",
  name: "Select transcriptor",
  description: "Transcription disabled",
  size_mb: 0,
  is_downloaded: true,
  is_downloading: false,
};

const MODEL_ORDER = [
  "parakeet-tdt-0.6b-v3",
  "parakeet-tdt-0.6b-v2",
  "moonshine-base",
  "small",
  "medium",
  "turbo",
  "large",
];

const sortModels = (list: TranscriptionModelInfo[]) => {
  const orderIndex = new Map(MODEL_ORDER.map((id, i) => [id, i]));
  return [...list].sort((a, b) => {
    const ai = orderIndex.get(a.id);
    const bi = orderIndex.get(b.id);
    if (ai !== undefined && bi !== undefined) return ai - bi;
    if (ai !== undefined) return -1;
    if (bi !== undefined) return 1;
    return a.name.localeCompare(b.name);
  });
};

export function useTranscriptionModels() {
  const { getSetting, updateSetting } = useSettings();
  const [models, setModels] = useState<TranscriptionModelInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentModelId, setCurrentModelId] = useState<string>("none");
  const [modelStatus, setModelStatus] = useState<ModelStatus>("unloaded");
  const [modelError, setModelError] = useState<string | null>(null);
  const [extractingModels, setExtractingModels] = useState<
    Record<string, true>
  >({});
  const [downloadProgress, setDownloadProgress] = useState<
    Record<string, DownloadProgressPayload>
  >({});
  const [pendingDownloads, setPendingDownloads] = useState<Record<string, true>>(
    {}
  );
  const [downloadStats, setDownloadStats] = useState<
    Record<string, DownloadStats>
  >({});

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<TranscriptionModelInfo[]>("get_available_models");
      // Filter out diarization-only models from the transcription model list
      const transcriptionModels = list.filter(
        (m) => !m.id.startsWith("diarize-")
      );
      setModels([NONE_OPTION, ...sortModels(transcriptionModels)]);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setModels([NONE_OPTION]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const loadCurrentModel = useCallback(async () => {
    try {
      const current = await invoke<string>("get_current_model");
      setCurrentModelId(current || "none");
    } catch (err) {
      console.error("Failed to load current model:", err);
    }
  }, []);

  useEffect(() => {
    loadCurrentModel();
  }, [loadCurrentModel]);

  const handleModelState = useCallback(
    (event: { payload: ModelStateEvent }) => {
      const { event_type, model_id, error } = event.payload;
      switch (event_type) {
        case "loading_started":
          setModelStatus("loading");
          setModelError(null);
          break;
        case "loading_completed":
          setModelStatus("ready");
          setModelError(null);
          if (model_id) setCurrentModelId(model_id);
          break;
        case "loading_failed":
          setModelStatus("error");
          setModelError(error || "Failed to load model");
          break;
        case "unloaded":
          setModelStatus("unloaded");
          setModelError(null);
          setCurrentModelId("none");
          break;
      }
    },
    []
  );

  const handleDownloadProgress = useCallback(
    (event: { payload: DownloadProgressPayload }) => {
      const now = Date.now();
      setPendingDownloads((prev) => {
        if (!(event.payload.model_id in prev)) return prev;
        const next = { ...prev };
        delete next[event.payload.model_id];
        return next;
      });
      setDownloadProgress((prev) => ({
        ...prev,
        [event.payload.model_id]: event.payload,
      }));
      setModelStatus("downloading");
      setDownloadStats((prev) => {
        const current = prev[event.payload.model_id];
        if (!current) {
          return {
            ...prev,
            [event.payload.model_id]: {
              startTime: now,
              lastUpdate: now,
              totalDownloaded: event.payload.downloaded,
              speed: 0,
            },
          };
        }
        const timeDiff = (now - current.lastUpdate) / 1000;
        const bytesDiff = event.payload.downloaded - current.totalDownloaded;
        if (timeDiff <= 0.5) {
          return prev;
        }
        const currentSpeed = bytesDiff / (1024 * 1024) / timeDiff;
        const validSpeed = Math.max(0, currentSpeed);
        const smoothedSpeed =
          current.speed > 0 ? current.speed * 0.8 + validSpeed * 0.2 : validSpeed;
        return {
          ...prev,
          [event.payload.model_id]: {
            startTime: current.startTime,
            lastUpdate: now,
            totalDownloaded: event.payload.downloaded,
            speed: Math.max(0, smoothedSpeed),
          },
        };
      });
    },
    []
  );

  const handleDownloadComplete = useCallback(() => {
    setPendingDownloads({});
    setDownloadProgress((prev) => {
      const next = { ...prev };
      Object.keys(next).forEach((k) => delete next[k]);
      return next;
    });
    setDownloadStats((prev) => {
      const next = { ...prev };
      Object.keys(next).forEach((k) => delete next[k]);
      return next;
    });
    setModelStatus(currentModelId === "none" ? "unloaded" : "ready");
    refresh();
  }, [refresh, currentModelId]);

  const handleExtractStart = useCallback((event: { payload: string }) => {
    setPendingDownloads((prev) => {
      if (!(event.payload in prev)) return prev;
      const next = { ...prev };
      delete next[event.payload];
      return next;
    });
    setDownloadProgress((prev) => {
      if (!(event.payload in prev)) return prev;
      const next = { ...prev };
      delete next[event.payload];
      return next;
    });
    setDownloadStats((prev) => {
      if (!(event.payload in prev)) return prev;
      const next = { ...prev };
      delete next[event.payload];
      return next;
    });
    setExtractingModels((prev) => ({ ...prev, [event.payload]: true }));
    setModelStatus("extracting");
  }, []);

  const handleExtractComplete = useCallback(
    (event: { payload: string }) => {
      setExtractingModels((prev) => {
        const next = { ...prev };
        delete next[event.payload];
        return next;
      });
      setModelStatus(currentModelId === "none" ? "unloaded" : "ready");
      refresh();
    },
    [refresh, currentModelId]
  );

  const handleExtractFailed = useCallback(
    (event: { payload: { model_id: string; error: string } }) => {
      setExtractingModels((prev) => {
        const next = { ...prev };
        delete next[event.payload.model_id];
        return next;
      });
      setModelStatus("error");
      setModelError(`Failed to extract model: ${event.payload.error}`);
    },
    []
  );

  // Setup Tauri listeners with proper lifecycle management
  useTauriListen<ModelStateEvent>("model-state-changed", handleModelState);
  useTauriListen<DownloadProgressPayload>("model-download-progress", handleDownloadProgress);
  useTauriListen<string>("model-download-complete", handleDownloadComplete);
  useTauriListen<string>("model-extraction-started", handleExtractStart);
  useTauriListen<string>("model-extraction-completed", handleExtractComplete);
  useTauriListen<{ model_id: string; error: string }>("model-extraction-failed", handleExtractFailed);

  const selected = getSetting("selected_transcription_model") || "none";
  const current =
    models.find((m) => m.id === selected) ?? models[0] ?? NONE_OPTION;

  const setActiveModel = useCallback(
    async (modelId: string) => {
      await updateSetting("selected_transcription_model", modelId);
      if (modelId === "none") {
        try {
          await invoke("set_active_model", { modelId: "none" });
        } catch (e) {
          console.warn("set_active_model failed:", e);
        }
        return;
      }
      try {
        await invoke("set_active_model", { modelId });
      } catch (e) {
        console.warn("set_active_model failed:", e);
      }
    },
    [updateSetting]
  );

  const downloadModel = useCallback(
    async (modelId: string) => {
      setPendingDownloads((prev) => ({ ...prev, [modelId]: true }));
      try {
        await invoke("download_model", { modelId });
      } catch (e) {
        setPendingDownloads((prev) => {
          if (!(modelId in prev)) return prev;
          const next = { ...prev };
          delete next[modelId];
          return next;
        });
        console.error("download_model failed:", e);
        throw e;
      }
    },
    []
  );

  const deleteModel = useCallback(async (modelId: string) => {
    try {
      await invoke("delete_model", { modelId });
      refresh();
      if (currentModelId === modelId) {
        setCurrentModelId("none");
      }
    } catch (e) {
      console.error("delete_model failed:", e);
      throw e;
    }
  }, [refresh, currentModelId]);

  const cancelDownload = useCallback(async (modelId: string) => {
    try {
      await invoke("cancel_download", { modelId });
      setPendingDownloads((prev) => {
        if (!(modelId in prev)) return prev;
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      setDownloadStats((prev) => {
        const next = { ...prev };
        delete next[modelId];
        return next;
      });
      refresh();
    } catch (e) {
      console.error("cancel_download failed:", e);
      throw e;
    }
  }, [refresh]);

  const progressValues = Object.values(downloadProgress);
  const statsValues = Object.values(downloadStats);
  const totalDownloaded = progressValues.reduce(
    (sum, p) => sum + p.downloaded,
    0
  );
  const totalSize = progressValues.reduce((sum, p) => sum + p.total, 0);
  const percentage =
    totalSize > 0 ? (totalDownloaded / totalSize) * 100 : 0;
  const speed = statsValues.reduce((sum, s) => sum + s.speed, 0);
  const activeDownloadIds = Object.keys(downloadProgress);
  const activeDownloadLabel =
    activeDownloadIds.length === 1
      ? models.find((m) => m.id === activeDownloadIds[0])?.name ||
        "Downloading"
      : `Downloading ${activeDownloadIds.length} models`;

  return {
    models,
    loading,
    error,
    currentModelId,
    modelStatus,
    modelError,
    extractingModels,
    refresh,
    selected,
    current,
    setActiveModel,
    downloadModel,
    deleteModel,
    downloadProgress,
    downloadStats,
    pendingDownloads,
    cancelDownload,
    downloadSummary: {
      active: progressValues.length > 0,
      percentage,
      speed,
      label: activeDownloadLabel,
      totalDownloaded,
      totalSize,
    },
  };
}
