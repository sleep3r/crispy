import React, { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Slider } from "../ui/Slider";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

export const MicrophoneVolume: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();

  const selectedMicrophone = getSetting("selected_microphone");
  const selectedOutputDevice = getSetting("selected_output_device") || "";
  const volume = Number.parseInt(getSetting("microphone_volume") || "100", 10);
  const selectedModel = getSetting("selected_model") || "dummy";

  const requestRef = useRef<number>();
  const lastLevel = useRef(0);
  const meterRef = useRef<HTMLDivElement>(null);
  const volumeRef = useRef(volume);
  const modelRef = useRef(selectedModel);

  // Keep latest volume available inside RAF loop
  volumeRef.current = volume;
  modelRef.current = selectedModel;

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    if (!selectedMicrophone) {
      lastLevel.current = 0;
      if (meterRef.current) meterRef.current.style.width = "0%";
      return;
    }

    const init = async () => {
      try {
        unlisten = await listen<number>("microphone-level", (event) => {
          const raw = event.payload;
          // macOS-like feel: ignore noise floor + faster peak
          const noiseFloor = 0.01;
          const gain = 5.2;
          const normalized = Math.max(0, raw - noiseFloor) / (1 - noiseFloor);
          const curved = Math.pow(Math.min(normalized * gain, 1), 0.3);
          let visual = Math.min(curved, 1);

          if (modelRef.current === "noisy") {
            const noiseBase = 0.08;
            const noiseJitter = (Math.random() - 0.5) * 0.06;
            visual = Math.min(Math.max(visual + noiseBase + noiseJitter, 0), 1);
          }

          lastLevel.current = lastLevel.current * 0.7 + visual * 0.3;
        });

        await invoke("start_monitoring", {
          deviceName: selectedMicrophone,
          outputDeviceName: selectedOutputDevice,
          modelName: selectedModel,
          volume: volume / 100,
        });
      } catch (error) {
        console.error("Failed to initialize monitoring:", error);
      }
    };

    init();

    const animate = () => {
      // DOM update only; no React state update
      const scaled = lastLevel.current * (volumeRef.current / 100);
      const pct = Math.min(scaled * 100, 100);
      if (meterRef.current) {
        // Smooth interpolation for better visual feedback
        const currentWidth = Number.parseFloat(meterRef.current.style.width) || 0;
        const targetWidth = pct;
        const newWidth = currentWidth + (targetWidth - currentWidth) * 0.3;
        meterRef.current.style.width = `${newWidth}%`;
      }
      requestRef.current = requestAnimationFrame(animate);
    };
    requestRef.current = requestAnimationFrame(animate);

    return () => {
      if (unlisten) unlisten();
      if (requestRef.current) cancelAnimationFrame(requestRef.current);
      invoke("stop_monitoring").catch(console.error);
    };
  }, [selectedMicrophone, selectedOutputDevice]);

  useEffect(() => {
    if (!selectedMicrophone) return;
    invoke("set_monitoring_volume", { volume: volume / 100 }).catch(
      console.error,
    );
  }, [selectedMicrophone, volume]);

  useEffect(() => {
    if (!selectedMicrophone) return;
    invoke("set_monitoring_model", { modelName: selectedModel }).catch(
      console.error,
    );
  }, [selectedMicrophone, selectedModel]);

  const handleVolumeChange = (value: number) => {
    updateSetting("microphone_volume", value.toString());
  };

  return (
    <SettingContainer
      title="Input Volume"
      description="Adjust microphone sensitivity and monitor levels."
    >
      <div className="flex flex-col gap-5">
        {/* Volume slider */}
        <div className="flex items-center gap-4">
          <Slider
            value={volume}
            min={0}
            max={100}
            step={1}
            onChange={handleVolumeChange}
            className="flex-1"
          />
          <span className="text-sm font-medium w-10 text-right tabular-nums">{volume}%</span>
        </div>

        {/* Input signal level (macOS-style segmented meter) */}
        <div className="flex flex-col gap-2">
          <span className="text-sm text-mid-gray">Input signal level</span>
          <div className="relative w-full h-4 rounded-md border border-mid-gray/10 bg-mid-gray/5 overflow-hidden">
            {/* Base segments */}
            <div
              className="absolute inset-0"
              style={{
                backgroundImage:
                  "repeating-linear-gradient(90deg, rgba(0,0,0,0.18) 0 6px, transparent 6px 10px)",
              }}
            />
            {/* Active segments */}
            <div
              ref={meterRef}
              className="absolute inset-y-0 left-0"
              style={{
                width: "0%",
                backgroundImage:
                  "repeating-linear-gradient(90deg, rgba(0,200,80,0.9) 0 6px, transparent 6px 10px)",
              }}
            />
          </div>
        </div>
      </div>
    </SettingContainer>
  );
};
