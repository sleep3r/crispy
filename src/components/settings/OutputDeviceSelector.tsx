import React from "react";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ResetButton } from "../ui/ResetButton";
import { useSettings } from "../../hooks/useSettings";

interface OutputDeviceSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const OutputDeviceSelector: React.FC<OutputDeviceSelectorProps> =
  React.memo(
    ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
      const {
        getSetting,
        updateSetting,
        resetSetting,
        isUpdating,
        isLoading,
        outputDevices,
        refreshOutputDevices,
      } = useSettings();

      const rawSelection = getSetting("selected_output_device");
      const selectedOutputDevice =
        rawSelection === "default" ? "Default" : rawSelection || null;

      const handleOutputDeviceSelect = async (deviceName: string) => {
        await updateSetting("selected_output_device", deviceName);
      };

      const handleReset = async () => {
        await resetSetting("selected_output_device");
      };

      const outputDeviceOptions = [
        { value: "Default", label: "System Default" },
        ...outputDevices.map((device) => ({
          value: device.name,
          label: device.name,
        })),
      ];

      return (
        <SettingContainer
          title="Output device"
          description="Choose where the cleaned audio is routed."
          descriptionMode={descriptionMode}
          grouped={grouped}
          disabled={disabled}
        >
          <div className="flex items-center space-x-1">
            <Dropdown
              options={outputDeviceOptions}
              selectedValue={selectedOutputDevice}
              onSelect={handleOutputDeviceSelect}
              placeholder={
                isLoading || outputDevices.length === 0
                  ? "Loading outputs..."
                  : "Choose an output"
              }
              disabled={
                disabled ||
                isUpdating("selected_output_device") ||
                isLoading ||
                outputDevices.length === 0
              }
              onRefresh={refreshOutputDevices}
            />
            <ResetButton
              onClick={handleReset}
              disabled={
                disabled || isUpdating("selected_output_device") || isLoading
              }
            />
          </div>
        </SettingContainer>
      );
    },
  );
