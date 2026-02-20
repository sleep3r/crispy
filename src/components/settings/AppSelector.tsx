import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";

interface RecordableApp {
  id: string;
  name: string;
  bundle_id: string;
}

interface AppSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const AppSelector: React.FC<AppSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
    const { getSetting, updateSetting, resetSetting } = useSettings();
    const [apps, setApps] = useState<RecordableApp[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const selectedApp = getSetting("selected_recording_app") || "none";

    useEffect(() => {
      loadApps();
    }, []);

    const loadApps = async () => {
      setIsLoading(true);
      setError(null);
      try {
        const appList = await invoke<RecordableApp[]>("get_recordable_apps");
        setApps(appList);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to load apps");
        console.error("Failed to load recordable apps:", err);
      } finally {
        setIsLoading(false);
      }
    };

    const handleAppSelect = async (bundleId: string) => {
      await updateSetting("selected_recording_app", bundleId);
    };

    const handleReset = async () => {
      await resetSetting("selected_recording_app");
    };

    // Use bundle_id as the option value so the selection persists across app restarts
    const appOptions = apps.map((app) => ({
      value: app.bundle_id,
      label: app.name,
    }));

    const getPlaceholder = () => {
      if (isLoading) return "Loading apps...";
      if (error) return "Error loading apps";
      return "Choose an app";
    };
    const placeholder = getPlaceholder();

    return (
      <SettingContainer
        title="App Audio Capture"
        description="Select an application to capture audio from during recording."
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center space-x-1">
          <Dropdown
            options={appOptions}
            selectedValue={selectedApp}
            onSelect={handleAppSelect}
            placeholder={placeholder}
            disabled={disabled || isLoading || !!error}
            onRefresh={loadApps}
          />
          <ResetButton
            onClick={handleReset}
            disabled={disabled || isLoading}
          />
        </div>
        {error && (
          <p className="text-xs text-red-500 mt-1">{error}</p>
        )}
      </SettingContainer>
    );
  },
);
