import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingsGroup } from "../../ui/SettingsGroup";
import {
  MicrophoneSelector,
  OutputDeviceSelector,
  MicrophoneVolume,
  RecordingControls,
} from "../";
import { AppSelector } from "../AppSelector";
import { useBlackHoleStatus } from "../../../hooks/useBlackHoleStatus";
import { useSettings } from "../../../hooks/useSettings";

export const GeneralSettings: React.FC = () => {
  const { status, isLoading, error, refresh } = useBlackHoleStatus();
  const { getSetting } = useSettings();
  const isBlackHoleInstalled = status?.installed ?? false;
  const hasBlackHole = (status?.paths?.length ?? 0) > 0;
  const showWarning = !isLoading && !isBlackHoleInstalled;
  const selectedOutput = getSetting("selected_output_device");
  const showOutputHint = !selectedOutput;
  const [currentPlatform, setCurrentPlatform] = useState<string>("");

  useEffect(() => {
    invoke<string>("get_platform")
      .then(setCurrentPlatform)
      .catch(() => setCurrentPlatform(""));
  }, []);

  const handleOpenVBAudio = async () => {
    try {
      await invoke("open_url", { url: "https://vb-audio.com/Cable/" });
    } catch (err) {
      console.error("Failed to open VB-Audio site:", err);
    }
  };

  return (
    <div className="flex flex-col gap-6 w-full max-w-3xl">
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">General Settings</h1>
        <p className="text-sm text-mid-gray">
          Configure your audio input and output devices.
        </p>
      </div>

      {error && (
        <div className="px-4 py-3 rounded-lg border border-red-500/30 bg-red-500/10 text-sm text-red-700">
          Failed to check BlackHole status: {error}
        </div>
      )}

      {showWarning && currentPlatform === "macos" && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm">
          <div className="font-medium text-blue-800">Audio routing setup (macOS)</div>
          <div className="mt-2 text-blue-700/90 space-y-1.5">
            <p>
              To route processed audio to other apps (Zoom, Teams, Discord, etc.), install <strong>BlackHole 2ch</strong> — a free virtual audio driver.
            </p>
            <p className="text-xs">
              <strong>Setup:</strong> Install BlackHole → Select it as Output Device in Crispy → 
              In your meeting app, select "BlackHole 2ch" as microphone input
            </p>
          </div>
          <div className="flex items-center gap-3 mt-3">
            <button
              type="button"
              onClick={async () => {
                try {
                  await invoke("open_url", { url: "https://existential.audio/blackhole/" });
                } catch (err) {
                  console.error("Failed to open BlackHole site:", err);
                }
              }}
              className="text-xs text-blue-700 hover:text-blue-800 font-medium underline"
            >
              Download BlackHole
            </button>
            <button
              type="button"
              onClick={refresh}
              className="text-xs text-blue-700/60 hover:text-blue-700 font-medium"
            >
              Recheck
            </button>
          </div>
        </div>
      )}

      {currentPlatform === "windows" && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm">
          <div className="font-medium text-blue-800">Audio routing setup (Windows)</div>
          <div className="mt-2 text-blue-700/90 space-y-1.5">
            <p>
              To route processed audio to other apps (Zoom, Teams, Discord, etc.), install <strong>VB-Audio Cable</strong> or <strong>VoiceMeeter</strong> — free virtual audio devices.
            </p>
            <p className="text-xs">
              <strong>Setup:</strong> Install VB-Audio Cable → Select "CABLE Input" as Output Device in Crispy → 
              In your meeting app, select "CABLE Output" as microphone input
            </p>
          </div>
          <button
            type="button"
            onClick={handleOpenVBAudio}
            className="mt-3 text-xs text-blue-700 hover:text-blue-800 font-medium underline"
          >
            Download VB-Audio Cable
          </button>
        </div>
      )}

      {currentPlatform === "linux" && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm">
          <div className="font-medium text-blue-800">Audio routing setup (Linux)</div>
          <div className="mt-2 text-blue-700/90 space-y-1.5">
            <p>
              To route processed audio to other apps (Zoom, Teams, Discord, etc.), create a loopback device using <strong>PulseAudio</strong> or <strong>PipeWire</strong>.
            </p>
            <p className="text-xs">
              <strong>PulseAudio:</strong> <code className="bg-blue-700/20 px-1 rounded">pactl load-module module-loopback</code>
            </p>
            <p className="text-xs">
              <strong>PipeWire:</strong> Use pw-loopback or configure virtual devices via pavucontrol
            </p>
            <p className="text-xs mt-1">
              Select the loopback device as Output in Crispy, then select the monitoring device in your meeting app.
            </p>
          </div>
        </div>
      )}

      {showOutputHint && !showWarning && currentPlatform !== "windows" && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm text-blue-800">
          <div className="font-medium">Output device not selected</div>
          <div className="mt-1 text-blue-700/90">
            {hasBlackHole
              ? "Please select an output device (BlackHole recommended) to route processed audio."
              : "Please select an output device to route processed audio."}
          </div>
        </div>
      )}

      <SettingsGroup
        title="Audio I/O"
        description="Pick your microphone input and output device."
      >
        <MicrophoneSelector grouped />
        <MicrophoneVolume />
        <OutputDeviceSelector grouped />
      </SettingsGroup>

      <SettingsGroup
        title="Recording"
        description="Record meetings with processed mic + app audio."
      >
        <AppSelector grouped />
        {currentPlatform === "windows" && (
          <div className="px-3 py-2 rounded-lg border border-blue-500/30 bg-blue-500/10 text-xs text-blue-800">
            <strong>Windows 10 2004+ required:</strong> App audio capture uses Process Loopback (requires Windows 10 build 19041 or newer). If capture fails, only microphone audio will be recorded.
          </div>
        )}
        <RecordingControls />
      </SettingsGroup>
    </div>
  );
};
