import React, { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { FileVideo, Upload, CheckCircle, XCircle, Loader2 } from "lucide-react";

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

  useEffect(() => {
    checkFFmpeg();

    // Listen for file drop events
    let unlistenDrop: UnlistenFn | undefined;
    let unlistenDrag: UnlistenFn | undefined;
    let unlistenCancelled: UnlistenFn | undefined;

    const setupFileDrop = async () => {
      unlistenDrop = await listen<{ paths: string[] } | string[]>("tauri://drag-drop", async (event) => {
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

      // Also listen for drag hover events
      unlistenDrag = await listen("tauri://drag", () => {
        setIsDragging(true);
      });

      unlistenCancelled = await listen("tauri://drag-cancelled", () => {
        setIsDragging(false);
      });
    };

    setupFileDrop();

    return () => {
      if (unlistenDrop) unlistenDrop();
      if (unlistenDrag) unlistenDrag();
      if (unlistenCancelled) unlistenCancelled();
    };
  }, [addConversionJob]);

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
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 space-y-3">
          <div>
            <p className="text-sm font-medium text-red-600">FFmpeg not installed</p>
            <p className="text-xs text-red-600/80 mt-1">
              FFmpeg is required for file conversion. Install it to enable this feature.
            </p>
          </div>
          
          <div className="space-y-2 text-xs text-red-600/80">
            <p className="font-medium">Installation:</p>
            <ul className="list-disc list-inside space-y-1 ml-2">
              <li><span className="font-mono">macOS:</span> brew install ffmpeg</li>
              <li><span className="font-mono">Ubuntu/Debian:</span> sudo apt install ffmpeg</li>
              <li><span className="font-mono">Windows:</span> Win+R, then winget install ffmpeg</li>
            </ul>
          </div>

          <div className="flex items-center gap-3">
            <a
              href="https://www.ffmpeg.org/download.html"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-xs text-red-600 hover:text-red-700 font-medium"
            >
              Download FFmpeg
              <span className="text-[10px]">↗</span>
            </a>
            <button
              type="button"
              onClick={checkFFmpeg}
              className="text-xs text-red-600/60 hover:text-red-600 font-medium"
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
