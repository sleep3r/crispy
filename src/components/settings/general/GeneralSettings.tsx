import React from "react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { MicrophoneSelector, OutputDeviceSelector, MicrophoneVolume, VirtualMicStatus } from "../";

export const GeneralSettings: React.FC = () => {
  return (
    <div className="flex flex-col gap-6 w-full max-w-3xl">
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">General Settings</h1>
        <p className="text-sm text-mid-gray">
          Configure your audio input and output devices.
        </p>
      </div>

      <SettingsGroup
        title="Audio I/O"
        description="Pick your microphone input and output device."
      >
        <MicrophoneSelector grouped />
        <MicrophoneVolume />
        <OutputDeviceSelector grouped />
      </SettingsGroup>

      <SettingsGroup
        title="Virtual Microphone (macOS)"
        description="Status of the Crispy virtual microphone output device."
      >
        <VirtualMicStatus />
      </SettingsGroup>
    </div>
  );
};
