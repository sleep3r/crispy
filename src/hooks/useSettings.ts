import { useEffect, useMemo, useState } from "react";

type SettingKey = "selected_microphone" | "selected_output_device";

interface AudioDevice {
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

  useEffect(() => {
    const listener = (nextState: SettingsState) => {
      setSettings(nextState);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  const audioDevices = useMemo<AudioDevice[]>(
    () => [
      { name: "Default" },
      { name: "Built-in Microphone" },
      { name: "USB Audio Interface" },
    ],
    [],
  );

  const outputDevices = useMemo<AudioDevice[]>(
    () => [
      { name: "Default" },
      { name: "System Output" },
      { name: "Virtual Cable" },
    ],
    [],
  );

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
    isLoading: false,
    audioDevices,
    outputDevices,
    refreshAudioDevices: () => {},
    refreshOutputDevices: () => {},
  };
};
