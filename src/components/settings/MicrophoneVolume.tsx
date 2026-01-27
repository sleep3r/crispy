import React, { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Slider } from "../../ui/Slider";
import { SettingContainer } from "../../ui/SettingContainer";
import { useSettings } from "../../../hooks/useSettings";

export const MicrophoneVolume: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();
  const [level, setLevel] = useState(0);
  const selectedMicrophone = getSetting("selected_microphone");
  // Default to 100% volume
  const volume = parseInt(getSetting("microphone_volume") || "100", 10);
  const requestRef = useRef<number>();
  const lastLevel = useRef(0);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const startMonitoring = async () => {
      try {
        if (selectedMicrophone) {
          await invoke("start_monitoring", { deviceName: selectedMicrophone });
        }
      } catch (error) {
        console.error("Failed to start monitoring:", error);
      }
    };

    const setupListener = async () => {
      unlisten = await listen<number>("microphone-level", (event) => {
        // Smooth the level a bit for visual clarity
        // Amplify the signal for better visualization (RMS is often low)
        const amplified = Math.min(event.payload * 5, 1); 
        lastLevel.current = lastLevel.current * 0.8 + amplified * 0.2;
      });
    };

    startMonitoring();
    setupListener();

    // Animation loop to update state less frequently than the event stream
    const animate = () => {
      setLevel(lastLevel.current);
      requestRef.current = requestAnimationFrame(animate);
    };
    requestRef.current = requestAnimationFrame(animate);

    return () => {
      if (unlisten) unlisten();
      if (requestRef.current) cancelAnimationFrame(requestRef.current);
      invoke("stop_monitoring").catch(console.error);
    };
  }, [selectedMicrophone]);

  const handleVolumeChange = (value: number) => {
    updateSetting("microphone_volume", value.toString());
  };

  return (
    <SettingContainer
      title="Input Volume"
      description="Adjust microphone sensitivity and monitor levels."
    >
      <div className="flex flex-col gap-4">
        {/* Visualizer Bar */}
        <div className="w-full h-4 bg-mid-gray/20 rounded-full overflow-hidden relative">
          <div
            className="h-full bg-text transition-all duration-75 ease-out"
            style={{ 
              width: `${Math.min(level * 100, 100)}%`,
              opacity: level > 0.01 ? 1 : 0.5 
            }}
          />
        </div>

        {/* Volume Slider */}
        <div className="flex items-center gap-4">
          <Slider
            value={volume}
            min={0}
            max={100}
            step={1}
            onChange={handleVolumeChange}
            className="flex-1"
          />
          <span className="text-sm font-medium w-8 text-right">{volume}%</span>
        </div>
      </div>
    </SettingContainer>
  );
};
