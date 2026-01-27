import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

type SettingKey = "selected_microphone" | "selected_output_device";

interface AudioDevice {
  id: string;
  name: string;
}

interface SettingsState {
  selected_microphone: string;
  selected_output_device: string;
}

const defaultSettings: SettingsState = {
  selected_microphone: "Default",
  selected_output_device: "Default",
};

let settingsState: SettingsState = { ...defaultSettings };
const listeners = new Set<(state: SettingsState) => void>();

const notify = () => {
  listeners.forEach((listener) => listener(settingsState));
};

const updateState = (partial: Partial<SettingsState>) => {
  settingsState = { ...settingsState, ...partial };
  notify();
};

export const useSettings = () => {
  const [settings, setSettings] = useState(settingsState);
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
  const [outputDevices, setOutputDevices] = useState<AudioDevice[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    const listener = (nextState: SettingsState) => {
      setSettings(nextState);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  const refreshAudioDevices = useCallback(async () => {
    setIsLoading(true);
    try {
      const devices = await invoke<AudioDevice[]>("get_input_devices");
      setAudioDevices(devices);
    } catch (error) {
      console.error("Failed to fetch input devices:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const refreshOutputDevices = useCallback(async () => {
    setIsLoading(true);
    try {
      const devices = await invoke<AudioDevice[]>("get_output_devices");
      setOutputDevices(devices);
    } catch (error) {
      console.error("Failed to fetch output devices:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Fetch devices on mount
  useEffect(() => {
    refreshAudioDevices();
    refreshOutputDevices();
  }, [refreshAudioDevices, refreshOutputDevices]);

  const getSetting = (key: SettingKey) => settings[key];

  const updateSetting = async (key: SettingKey, value: string) => {
    updateState({ [key]: value });
  };

  const resetSetting = async (key: SettingKey) => {
    updateState({ [key]: defaultSettings[key] });
  };

  const isUpdating = (_key: SettingKey) => false;

  return {
    settings,
    getSetting,
    updateSetting,
    resetSetting,
    isUpdating,
    isLoading,
    audioDevices,
    outputDevices,
    refreshAudioDevices,
    refreshOutputDevices,
  };
};
