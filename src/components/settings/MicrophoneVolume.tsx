import React, { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Slider } from "../ui/Slider";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { useTauriListen } from "../../hooks/useTauriListen";

let cachedSystemVolume: number | null = null;
let cachedSystemVolumeSupported: boolean | null = null;

const getInitialSystemVolumeState = () => {
  if (cachedSystemVolumeSupported === true && cachedSystemVolume != null) {
    return "ready";
  }
  if (cachedSystemVolumeSupported === false) {
    return "unsupported";
  }
  return "loading";
};

export const MicrophoneVolume: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();

  const selectedMicrophone = getSetting("selected_microphone");
  const selectedOutputDevice = getSetting("selected_output_device") || "";
  const volume = Number.parseInt(getSetting("microphone_volume") || "100", 10);
  const selectedModel = getSetting("selected_model") || "dummy";

  const [systemVolumeState, setSystemVolumeState] = useState<
    "loading" | "ready" | "unsupported"
  >(getInitialSystemVolumeState());
  const [systemVolume, setSystemVolume] = useState(
    cachedSystemVolume ?? 100,
  );

  const requestRef = useRef<number>();
  const lastLevel = useRef(0);
  const meterRef = useRef<HTMLDivElement>(null);
  const volumeRef = useRef(volume);
  const modelRef = useRef(selectedModel);

  // Keep latest volume available inside RAF loop
  volumeRef.current = volume;
  modelRef.current = selectedModel;

  // Setup global microphone level listener with proper lifecycle
  useTauriListen<number>("microphone-level", (event) => {
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

  // Start monitoring when selected devices change.
  useEffect(() => {
    if (!selectedMicrophone) {
      lastLevel.current = 0;
      if (meterRef.current) meterRef.current.style.width = "0%";
      return;
    }

    const init = async () => {
      try {
        const effectiveVolume = systemVolumeState === "ready" ? 1 : volume / 100;
        await invoke("start_monitoring", {
          deviceName: selectedMicrophone,
          outputDeviceName: selectedOutputDevice,
          modelName: selectedModel,
          volume: effectiveVolume,
        });
      } catch (error) {
        console.error("Failed to initialize monitoring:", error);
      }
    };

    init();

    const animate = () => {
      // DOM update only; no React state update
      const scaled =
        lastLevel.current *
        (systemVolumeState === "ready" ? 1 : volumeRef.current / 100);
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
      if (requestRef.current) cancelAnimationFrame(requestRef.current);
    };
  }, [selectedMicrophone, selectedOutputDevice]);

  // Keep monitoring alive across navigation; only stop when no microphone is selected.
  useEffect(() => {
    if (selectedMicrophone) return;
    invoke("stop_monitoring").catch(console.error);
  }, [selectedMicrophone]);

  useEffect(() => {
    if (!selectedMicrophone) return;
    const effectiveVolume = systemVolumeState === "ready" ? 1 : volume / 100;
    invoke("set_monitoring_volume", { volume: effectiveVolume }).catch(
      console.error,
    );
  }, [selectedMicrophone, volume, systemVolumeState]);

  useEffect(() => {
    if (!selectedMicrophone) return;
    invoke("set_monitoring_model", { modelName: selectedModel }).catch(
      console.error,
    );
  }, [selectedMicrophone, selectedModel]);

  useEffect(() => {
    invoke<number>("get_system_input_volume")
      .then((v) => {
        cachedSystemVolume = v;
        cachedSystemVolumeSupported = true;
        setSystemVolume(v);
        setSystemVolumeState("ready");
      })
      .catch(() => {
        cachedSystemVolumeSupported = false;
        setSystemVolumeState("unsupported");
      });
  }, []);

  const handleVolumeChange = (value: number) => {
    if (systemVolumeState === "ready") {
      setSystemVolume(value);
      invoke("set_system_input_volume", { volume: value }).catch(console.error);
    } else {
      updateSetting("microphone_volume", value.toString());
    }
  };

  const displayVolume = systemVolumeState === "ready" ? systemVolume : volume;

  return (
    <SettingContainer
      title="Input Volume"
      description="Adjust microphone sensitivity and monitor levels."
    >
      <div className="flex flex-col gap-5">
        {/* Volume slider */}
        {systemVolumeState === "loading" ? (
          <div className="flex items-center gap-4 opacity-60">
            <div className="flex-1 h-9 rounded-md border border-mid-gray/20 bg-mid-gray/10" />
            <span className="text-sm font-medium w-10 text-right tabular-nums">—</span>
          </div>
        ) : (
          <div className="flex items-center gap-4">
            <Slider
              value={displayVolume}
              min={0}
              max={100}
              step={1}
              onChange={handleVolumeChange}
              className="flex-1"
            />
            <span className="text-sm font-medium w-10 text-right tabular-nums">
              {displayVolume}%
            </span>
          </div>
        )}

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
