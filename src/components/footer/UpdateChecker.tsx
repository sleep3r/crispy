import { useState, useEffect } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download } from "lucide-react";

export function UpdateChecker() {
  const [isChecking, setIsChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [isInstalling, setIsInstalling] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Check for updates on mount
    checkForUpdates();
  }, []);

  const checkForUpdates = async () => {
    if (isChecking || isInstalling) return;

    try {
      setIsChecking(true);
      setError(null);
      const availableUpdate = await check();

      if (availableUpdate?.available) {
        setUpdate(availableUpdate);
        console.log(`Update available: ${availableUpdate.version}`);
      } else {
        setUpdate(null);
      }
    } catch (err) {
      console.error("Failed to check for updates:", err);
      setError("Failed to check for updates");
    } finally {
      setIsChecking(false);
    }
  };

  const installUpdate = async () => {
    if (!update || isInstalling) return;

    try {
      setIsInstalling(true);
      setDownloadProgress(0);
      setError(null);

      let downloadedBytes = 0;
      let contentLength = 0;

      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            console.log(`Download started (${(contentLength / 1024 / 1024).toFixed(1)} MB)`);
            break;
          case "Progress":
            downloadedBytes += event.data.chunkLength;
            if (contentLength > 0) {
              const progress = Math.round((downloadedBytes / contentLength) * 100);
              setDownloadProgress(Math.min(progress, 100));
            }
            break;
          case "Finished":
            console.log("Download finished");
            setDownloadProgress(100);
            break;
        }
      });

      console.log("Update installed, relaunching...");
      await relaunch();
    } catch (err) {
      console.error("Failed to install update:", err);
      setError("Failed to install update");
      setIsInstalling(false);
      setDownloadProgress(0);
    }
  };

  // Installing state
  if (isInstalling) {
    return (
      <button
        type="button"
        className="inline-flex items-center gap-2 text-mid-gray cursor-default"
        disabled
      >
        <span className="w-2 h-2 rounded-full bg-logo-primary animate-pulse" />
        <span>
          Installing update: {downloadProgress}%
        </span>
      </button>
    );
  }

  // Update available state
  if (update) {
    return (
      <button
        type="button"
        onClick={installUpdate}
        className="inline-flex items-center gap-2 text-logo-primary hover:text-logo-primary/80 transition-colors"
      >
        <Download className="w-3 h-3" />
        <span>Update to v{update.version}</span>
      </button>
    );
  }

  // Error state
  if (error) {
    return (
      <button
        type="button"
        onClick={checkForUpdates}
        className="inline-flex items-center gap-2 text-mid-gray hover:text-text transition-colors"
      >
        <span>Check for updates</span>
      </button>
    );
  }

  // Checking state
  if (isChecking) {
    return (
      <span className="inline-flex items-center gap-2 text-mid-gray">
        <span className="w-2 h-2 rounded-full bg-mid-gray animate-pulse" />
        <span>Checking...</span>
      </span>
    );
  }

  // Default state - manual check button
  return (
    <button
      type="button"
      onClick={checkForUpdates}
      className="inline-flex items-center gap-2 text-mid-gray hover:text-text transition-colors"
    >
      <span>Check for updates</span>
    </button>
  );
}
