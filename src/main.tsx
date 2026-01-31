import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { TranscriptionResultView } from "./components/TranscriptionResultView";
import { TrayPopupView } from "./components/TrayPopupView";
import "./App.css";

function Root() {
  const [windowKind, setWindowKind] = useState<"main" | "transcription-result" | "tray-popup" | null>(null);

  useEffect(() => {
    const label = getCurrentWindow().label;
    if (label === "transcription-result") setWindowKind("transcription-result");
    else if (label === "tray-popup") setWindowKind("tray-popup");
    else setWindowKind("main");
  }, []);

  if (windowKind === null) {
    return (
      <div className="h-screen flex items-center justify-center bg-background text-mid-gray text-sm">
        Loading…
      </div>
    );
  }
  if (windowKind === "transcription-result") return <TranscriptionResultView />;
  if (windowKind === "tray-popup") return <TrayPopupView />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);

// Hide loading screen after React mounts
setTimeout(() => {
  document.body.classList.add("loaded");
}, 100);
