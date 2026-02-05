import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

interface AutostartToggleProps {
  grouped?: boolean;
}

export const AutostartToggle: React.FC<AutostartToggleProps> = ({
  grouped = false,
}) => {
  const { settings } = useSettings();
  const [isUpdating, setIsUpdating] = useState(false);
  const [localEnabled, setLocalEnabled] = useState<boolean | null>(null);
  
  // Use local state if set (during/after update), otherwise use settings value
  const autostartEnabled = localEnabled ?? (settings.autostart_enabled === "true");

  const handleToggle = async () => {
    if (isUpdating) return;

    const newValue = !autostartEnabled;
    setLocalEnabled(newValue);
    setIsUpdating(true);
    
    try {
      await invoke("set_autostart", { enabled: newValue });
    } catch (error) {
      console.error("Failed to update autostart setting:", error);
      // Revert local state on error
      setLocalEnabled(autostartEnabled);
    } finally {
      setIsUpdating(false);
    }
  };

  return (
    <SettingContainer
      title="Launch at Login"
      description="Automatically start Crispy when you log in to your computer."
      grouped={grouped}
      layout="horizontal"
    >
      <label className="relative inline-flex items-center cursor-pointer" aria-label="Toggle autostart">
        <input
          type="checkbox"
          checked={autostartEnabled}
          onChange={handleToggle}
          disabled={isUpdating}
          className="sr-only peer"
          aria-label="Launch at login"
        />
        <div className="w-11 h-6 bg-mid-gray/20 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-logo-primary/50 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-mid-gray/20 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-logo-primary disabled:opacity-50 disabled:cursor-not-allowed" />
      </label>
    </SettingContainer>
  );
};
