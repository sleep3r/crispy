import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

type SettingKey =
  | "selected_microphone"
  | "selected_output_device"
  | "microphone_volume"
  | "selected_model"
  | "selected_transcription_model"
  | "selected_recording_app"
  | "autostart_enabled"
  | "diarization_enabled";

interface AudioDevice {
  id: string;
  name: string;
}

interface SettingsState {
  selected_microphone: string;
  selected_output_device: string;
  microphone_volume: string;
  selected_model: string;
  selected_transcription_model: string;
  selected_recording_app: string;
  autostart_enabled: string;
  diarization_enabled: string;
}

const defaultSettings: SettingsState = {
  selected_microphone: "",
  selected_output_device: "",
  microphone_volume: "100",
  selected_model: "dummy",
  selected_transcription_model: "none",
  selected_recording_app: "none",
  autostart_enabled: "false",
  diarization_enabled: "false",
};

let settingsState: SettingsState = { ...defaultSettings };
const listeners = new Set<(state: SettingsState) => void>();

// Shared device state
let cachedAudioDevices: AudioDevice[] = [];
let cachedOutputDevices: AudioDevice[] = [];
let didInitDevices = false;
const deviceListeners = new Set<() => void>();
let settingsInitPromise: Promise<void> | null = null;

const notifyDeviceListeners = () => {
  deviceListeners.forEach((listener) => listener());
};

const notify = () => {
  listeners.forEach((listener) => listener(settingsState));
};

const updateState = (partial: Partial<SettingsState>) => {
  settingsState = { ...settingsState, ...partial };
  notify();
};

const ensureSettingsLoaded = async () => {
  if (settingsInitPromise) return settingsInitPromise;
  settingsInitPromise = (async () => {
    try {
      const saved = await invoke<Partial<SettingsState>>("get_app_settings");
      updateState({ ...defaultSettings, ...saved });
    } catch (error) {
      console.error("Failed to load app settings:", error);
    }
  })();
  return settingsInitPromise;
};

export const useSettings = () => {
  const [settings, setSettings] = useState(settingsState);
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>(cachedAudioDevices);
  const [outputDevices, setOutputDevices] = useState<AudioDevice[]>(cachedOutputDevices);
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

  useEffect(() => {
    ensureSettingsLoaded();
  }, []);

  useEffect(() => {
    const deviceListener = () => {
      setAudioDevices(cachedAudioDevices);
      setOutputDevices(cachedOutputDevices);
    };
    deviceListeners.add(deviceListener);
    return () => {
      deviceListeners.delete(deviceListener);
    };
  }, []);

  const refreshAudioDevices = useCallback(async () => {
    setIsLoading(true);
    try {
      const devices = await invoke<AudioDevice[]>("get_input_devices");
      cachedAudioDevices = devices;
      setAudioDevices(devices);
      notifyDeviceListeners();
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
      cachedOutputDevices = devices;
      setOutputDevices(devices);
      notifyDeviceListeners();
    } catch (error) {
      console.error("Failed to fetch output devices:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const initializeDefaultDevices = useCallback(async () => {
    try {
      await ensureSettingsLoaded();
      const defaults = await invoke<{
        default_input: string | null;
        blackhole_output: string | null;
      }>("get_default_devices");

      // Set default input if not already set
      if (!settingsState.selected_microphone && defaults.default_input) {
        updateState({ selected_microphone: defaults.default_input });
        invoke("set_app_setting", {
          key: "selected_microphone",
          value: defaults.default_input,
        }).catch(console.error);
      }

      // Set BlackHole output if found and not already set
      if (!settingsState.selected_output_device && defaults.blackhole_output) {
        updateState({ selected_output_device: defaults.blackhole_output });
        invoke("set_app_setting", {
          key: "selected_output_device",
          value: defaults.blackhole_output,
        }).catch(console.error);
      }
    } catch (error) {
      console.error("Failed to initialize default devices:", error);
    }
  }, []);

  // Fetch devices only once per app session
  useEffect(() => {
    if (didInitDevices) return;
    didInitDevices = true;
    
    const init = async () => {
      await ensureSettingsLoaded();
      await Promise.all([refreshAudioDevices(), refreshOutputDevices()]);
      await initializeDefaultDevices();
    };
    
    init();
  }, [refreshAudioDevices, refreshOutputDevices, initializeDefaultDevices]);

  const getSetting = (key: SettingKey) => settings[key];

  const updateSetting = async (key: SettingKey, value: string) => {
    updateState({ [key]: value });
    try {
      await invoke("set_app_setting", { key, value });
    } catch (error) {
      console.error("Failed to persist setting:", error);
    }
  };

  const resetSetting = async (key: SettingKey) => {
    updateState({ [key]: defaultSettings[key] });
    try {
      await invoke("set_app_setting", { key, value: defaultSettings[key] });
    } catch (error) {
      console.error("Failed to persist setting reset:", error);
    }
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
