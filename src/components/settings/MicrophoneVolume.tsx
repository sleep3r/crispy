import React, { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Slider } from "../ui/Slider";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

export const MicrophoneVolume: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();

  const selectedMicrophone = getSetting("selected_microphone");
  const volume = parseInt(getSetting("microphone_volume") || "100", 10);

  const requestRef = useRef<number>();
  const lastLevel = useRef(0);
  const barRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    if (!selectedMicrophone) {
      lastLevel.current = 0;
      if (barRef.current) barRef.current.style.width = "0%";
      return;
    }

    const init = async () => {
      try {
        unlisten = await listen<number>("microphone-level", (event) => {
          const raw = event.payload;
          const visual = Math.min(Math.sqrt(raw) * 1.5, 1);
          lastLevel.current = lastLevel.current * 0.7 + visual * 0.3;
        });

        await invoke("start_monitoring", { deviceName: selectedMicrophone });
      } catch (error) {
        console.error("Failed to initialize monitoring:", error);
      }
    };

    init();

    const animate = () => {
      // DOM update only; no React state update
      const pct = Math.min(lastLevel.current * 100, 100);
      if (barRef.current) barRef.current.style.width = `${pct}%`;
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
        <div className="w-full h-4 bg-mid-gray/10 rounded-full overflow-hidden relative border border-mid-gray/20">
          <div
            ref={barRef}
            className="h-full bg-gradient-to-r from-green-500 to-green-600 transition-[width] duration-100 ease-out"
            style={{ width: "0%" }}
          />
        </div>

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
