import React, { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { FileVideo, Upload, CheckCircle, XCircle, Loader2, Copy, Check, ExternalLink } from "lucide-react";
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
  const [copiedCommand, setCopiedCommand] = useState<string | null>(null);
  const recentDropsRef = useRef<Map<string, number>>(new Map());

  const copyToClipboard = async (text: string, id: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedCommand(id);
      setTimeout(() => setCopiedCommand(null), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  const checkFFmpeg = async () => {
    try {
      const available = await invoke<boolean>("check_ffmpeg");
      setFfmpegAvailable(available);
    } catch {
      setFfmpegAvailable(false);
    }
  };

  const handleOpenFfmpegGuide = async () => {
    try {
      await invoke("open_url", { url: "https://github.com/oop7/ffmpeg-install-guide" });
    } catch (err) {
      console.error("Failed to open FFmpeg guide:", err);
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

      {/* FFmpeg Installation Guide */}
      {ffmpegAvailable === false && (
        <div className="px-4 py-4 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm space-y-3">
          <div>
            <div className="font-semibold text-blue-800 mb-1">FFmpeg Required</div>
            <p className="text-blue-700/90 text-xs">
              FFmpeg is required to convert audio and video files. Follow the instructions below for your platform.
            </p>
          </div>

          {/* macOS Instructions */}
          <div className="space-y-2 pt-2 border-t border-blue-500/20">
            <p className="font-medium text-blue-800 text-xs">macOS Installation (Recommended):</p>
            <div className="space-y-2">
              <div className="bg-blue-700/10 rounded-md p-2 space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <code className="text-[11px] font-mono text-blue-900 flex-1 break-all">
                    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
                  </code>
                  <button
                    type="button"
                    onClick={() => copyToClipboard('/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"', 'brew-install')}
                    className="shrink-0 p-1 text-blue-700 hover:text-blue-800 hover:bg-blue-700/10 rounded transition-colors"
                    title="Copy command"
                  >
                    {copiedCommand === 'brew-install' ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </div>
                <p className="text-[10px] text-blue-700/70">1. Install Homebrew (if not installed)</p>
              </div>
              <div className="bg-blue-700/10 rounded-md p-2 space-y-1.5">
                <div className="flex items-center justify-between gap-2">
                  <code className="text-[11px] font-mono text-blue-900">brew install ffmpeg</code>
                  <button
                    type="button"
                    onClick={() => copyToClipboard('brew install ffmpeg', 'brew-ffmpeg')}
                    className="shrink-0 p-1 text-blue-700 hover:text-blue-800 hover:bg-blue-700/10 rounded transition-colors"
                    title="Copy command"
                  >
                    {copiedCommand === 'brew-ffmpeg' ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </div>
                <p className="text-[10px] text-blue-700/70">2. Install FFmpeg via Homebrew</p>
              </div>
            </div>
          </div>

          {/* Linux Instructions */}
          <div className="space-y-2 pt-2 border-t border-blue-500/20">
            <p className="font-medium text-blue-800 text-xs">Linux Installation:</p>
            <div className="space-y-1.5">
              <div className="bg-blue-700/10 rounded-md p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex-1">
                    <span className="text-[10px] text-blue-700/70 block mb-1">Ubuntu/Debian:</span>
                    <code className="text-[11px] font-mono text-blue-900">sudo apt update && sudo apt install ffmpeg</code>
                  </div>
                  <button
                    type="button"
                    onClick={() => copyToClipboard('sudo apt update && sudo apt install ffmpeg', 'apt-ffmpeg')}
                    className="shrink-0 p-1 text-blue-700 hover:text-blue-800 hover:bg-blue-700/10 rounded transition-colors"
                    title="Copy command"
                  >
                    {copiedCommand === 'apt-ffmpeg' ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </div>
              </div>
              <div className="bg-blue-700/10 rounded-md p-2">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex-1">
                    <span className="text-[10px] text-blue-700/70 block mb-1">Fedora:</span>
                    <code className="text-[11px] font-mono text-blue-900">sudo dnf install ffmpeg</code>
                  </div>
                  <button
                    type="button"
                    onClick={() => copyToClipboard('sudo dnf install ffmpeg', 'dnf-ffmpeg')}
                    className="shrink-0 p-1 text-blue-700 hover:text-blue-800 hover:bg-blue-700/10 rounded transition-colors"
                    title="Copy command"
                  >
                    {copiedCommand === 'dnf-ffmpeg' ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                </div>
              </div>
            </div>
          </div>

          {/* Windows Instructions */}
          <div className="space-y-2 pt-2 border-t border-blue-500/20">
            <p className="font-medium text-blue-800 text-xs">Windows Installation:</p>
            <div className="bg-blue-700/10 rounded-md p-2 space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <code className="text-[11px] font-mono text-blue-900">winget install ffmpeg</code>
                <button
                  type="button"
                  onClick={() => copyToClipboard('winget install ffmpeg', 'winget-ffmpeg')}
                  className="shrink-0 p-1 text-blue-700 hover:text-blue-800 hover:bg-blue-700/10 rounded transition-colors"
                  title="Copy command"
                >
                  {copiedCommand === 'winget-ffmpeg' ? <Check size={14} /> : <Copy size={14} />}
                </button>
              </div>
              <p className="text-[10px] text-blue-700/70">Press Win+R, type "cmd", paste this command</p>
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex items-center gap-3 pt-2 border-t border-blue-500/20">
            <button
              type="button"
              onClick={handleOpenFfmpegGuide}
              className="flex items-center gap-1.5 text-xs text-blue-700 hover:text-blue-800 font-medium transition-colors"
            >
              <ExternalLink size={12} />
              <span>Detailed Installation Guide</span>
            </button>
            <button
              type="button"
              onClick={checkFFmpeg}
              className="text-xs text-blue-700/60 hover:text-blue-700 font-medium transition-colors"
            >
              Recheck Installation
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
