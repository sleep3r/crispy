import React, { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FileVideo, Upload, CheckCircle, XCircle, Loader2 } from "lucide-react";
import { useTauriListen } from "../../hooks/useTauriListen";

interface ConversionJob {
  id: string;
  filename: string;
  status: "pending" | "converting" | "completed" | "error";
  error?: string;
}

export const ConvertView: React.FC = () => {
  const [jobs, setJobs] = useState<ConversionJob[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [ffmpegAvailable, setFfmpegAvailable] = useState<boolean | null>(null);
  const recentDropsRef = useRef<Map<string, number>>(new Map());

  const checkFFmpeg = async () => {
    try {
      const available = await invoke<boolean>("check_ffmpeg");
      setFfmpegAvailable(available);
    } catch {
      setFfmpegAvailable(false);
    }
  };

  const handleOpenFfmpegSite = async () => {
    try {
      await invoke("open_url", { url: "https://www.ffmpeg.org/download.html" });
    } catch (err) {
      console.error("Failed to open FFmpeg site:", err);
    }
  };

  const addConversionJob = useCallback(async (filePath: string) => {
    const filename = filePath.split(/[/\\]/).pop() || "unknown";
    const jobId = `${Date.now()}-${Math.random()}`;

    const newJob: ConversionJob = {
      id: jobId,
      filename,
      status: "pending",
    };

    setJobs((prev) => [...prev, newJob]);

    // Start conversion
    setJobs((prev) =>
      prev.map((j) => (j.id === jobId ? { ...j, status: "converting" as const } : j))
    );

    try {
      await invoke("convert_to_wav", { inputPath: filePath });
      setJobs((prev) =>
        prev.map((j) => (j.id === jobId ? { ...j, status: "completed" as const } : j))
      );
    } catch (err) {
      setJobs((prev) =>
        prev.map((j) =>
          j.id === jobId
            ? {
                ...j,
                status: "error" as const,
                error: err instanceof Error ? err.message : "Conversion failed",
              }
            : j
        )
      );
    }
  }, []);

  const handleFileSelect = async () => {
    try {
      const files = await open({
        multiple: true,
        filters: [
          {
            name: "Media Files",
            extensions: ["mp4", "mp3", "m4a", "mov", "avi", "mkv", "flac", "ogg", "webm"],
          },
        ],
      });

      if (!files) return;

      const selectedFiles = Array.isArray(files) ? files : [files];
      for (const filePath of selectedFiles) {
        await addConversionJob(filePath);
      }
    } catch (err) {
      console.error("Failed to select files:", err);
    }
  };

  // Check FFmpeg availability on mount
  useEffect(() => {
    checkFFmpeg();
  }, []);

  // Setup drag/drop listeners with proper lifecycle
  useTauriListen<{ paths: string[] } | string[]>("tauri://drag-drop", async (event) => {
    // Handle payload: can be { paths: string[] } (Tauri v2) or just string[]
    const payload = event.payload;
    let droppedPaths: string[] = [];

    if (Array.isArray(payload)) {
      droppedPaths = payload;
    } else if (payload && typeof payload === 'object' && 'paths' in payload && Array.isArray(payload.paths)) {
      droppedPaths = payload.paths;
    }

    const now = Date.now();
    const uniquePaths = Array.from(new Set(droppedPaths));
    const acceptedPaths = uniquePaths.filter((path) => {
      const last = recentDropsRef.current.get(path);
      if (last && now - last < 2000) return false;
      recentDropsRef.current.set(path, now);
      return true;
    });

    if (recentDropsRef.current.size > 50) {
      for (const [path, ts] of recentDropsRef.current) {
        if (now - ts > 10000) {
          recentDropsRef.current.delete(path);
        }
      }
    }

    for (const path of acceptedPaths) {
      await addConversionJob(path);
    }
    setIsDragging(false);
  });

  // Drag hover events
  useTauriListen("tauri://drag", () => {
    setIsDragging(true);
  });

  useTauriListen("tauri://drag-cancelled", () => {
    setIsDragging(false);
  });

  const clearCompleted = () => {
    setJobs((prev) => prev.filter((j) => j.status !== "completed"));
  };

  return (
    <div className="max-w-4xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        <h1 className="text-2xl font-semibold">Convert to WAV</h1>
        <p className="text-sm text-mid-gray">
          Convert audio/video files to WAV format and save them to your Recordings folder
        </p>
      </div>

      {/* FFmpeg Warning */}
      {ffmpegAvailable === false && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm">
          <div className="font-medium text-blue-800">FFmpeg required</div>
          <div className="mt-2 text-blue-700/90 space-y-1.5">
            <p>
              FFmpeg is required to convert audio and video files. Install it to enable this feature.
            </p>
            <div className="text-xs space-y-1 mt-2">
              <p className="font-medium text-blue-800">Installation:</p>
              <ul className="space-y-0.5 ml-3">
                <li>
                  <strong>macOS:</strong> <code className="bg-blue-700/20 px-1 rounded">brew install ffmpeg</code>
                </li>
                <li>
                  <strong>Ubuntu/Debian:</strong> <code className="bg-blue-700/20 px-1 rounded">sudo apt install ffmpeg</code>
                </li>
                <li>
                  <strong>Windows:</strong> Press Win+R, then run <code className="bg-blue-700/20 px-1 rounded">winget install ffmpeg</code>
                </li>
              </ul>
            </div>
          </div>
          <div className="flex items-center gap-3 mt-3">
            <button
              type="button"
              onClick={handleOpenFfmpegSite}
              className="text-xs text-blue-700 hover:text-blue-800 font-medium underline"
            >
              Download FFmpeg
            </button>
            <button
              type="button"
              onClick={checkFFmpeg}
              className="text-xs text-blue-700/60 hover:text-blue-700 font-medium"
            >
              Recheck
            </button>
          </div>
        </div>
      )}


      {/* Drop Zone */}
      <div
        className={`border-2 border-dashed rounded-lg p-12 flex flex-col items-center justify-center gap-4 transition-colors ${
          ffmpegAvailable === false
            ? "opacity-50 cursor-not-allowed"
            : isDragging
            ? "border-slider-fill bg-slider-fill/5 cursor-pointer"
            : "border-mid-gray/20 hover:border-mid-gray/40 cursor-pointer"
        }`}
        onClick={ffmpegAvailable !== false ? handleFileSelect : undefined}
      >
        <Upload size={48} className="text-mid-gray" />
        <div className="text-center space-y-2">
          {ffmpegAvailable === false ? (
            <>
              <p className="text-sm font-medium text-mid-gray/60">Install FFmpeg to enable conversion</p>
              <p className="text-xs text-mid-gray/60">See instructions above</p>
            </>
          ) : (
            <>
              <p className="text-sm font-medium">Click here to select files</p>
              <p className="text-xs text-mid-gray">or drag and drop (desktop only)</p>
            </>
          )}
        </div>
        {ffmpegAvailable !== false && (
          <p className="text-xs text-mid-gray">
            Supported: MP4, MP3, M4A, MOV, AVI, MKV, FLAC, OGG, WebM
          </p>
        )}
      </div>

      {/* Conversion Queue */}
      {jobs.length > 0 && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-medium text-mid-gray uppercase tracking-wide">
              Conversion Queue ({jobs.length})
            </h2>
            {jobs.some((j) => j.status === "completed") && (
              <button
                type="button"
                onClick={clearCompleted}
                className="text-xs text-mid-gray hover:text-text transition-colors"
              >
                Clear Completed
              </button>
            )}
          </div>

          <div className="space-y-2">
            {jobs.map((job) => (
              <div
                key={job.id}
                className="flex items-center gap-3 p-3 bg-background border border-mid-gray/20 rounded-lg"
              >
                <div className="shrink-0">
                  {job.status === "pending" && <FileVideo size={20} className="text-mid-gray" />}
                  {job.status === "converting" && (
                    <Loader2 size={20} className="text-slider-fill animate-spin" />
                  )}
                  {job.status === "completed" && (
                    <CheckCircle size={20} className="text-green-500" />
                  )}
                  {job.status === "error" && <XCircle size={20} className="text-red-500" />}
                </div>

                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium truncate">{job.filename}</p>
                  {job.status === "converting" && (
                    <p className="text-xs text-mid-gray">Converting...</p>
                  )}
                  {job.status === "completed" && (
                    <p className="text-xs text-green-600">Saved to Recordings</p>
                  )}
                  {job.status === "error" && (
                    <p className="text-xs text-red-500">{job.error || "Failed"}</p>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
