import React from "react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import {
  MicrophoneSelector,
  OutputDeviceSelector,
  MicrophoneVolume,
  RecordingControls,
} from "../";
import { useBlackHoleStatus } from "../../../hooks/useBlackHoleStatus";
import { useSettings } from "../../../hooks/useSettings";

export const GeneralSettings: React.FC = () => {
  const { status, isLoading, error, refresh } = useBlackHoleStatus();
  const { getSetting } = useSettings();
  const isBlackHoleInstalled = status?.installed ?? false;
  const showWarning = !isLoading && !isBlackHoleInstalled;
  const selectedOutput = getSetting("selected_output_device");
  const showOutputHint = !selectedOutput;

  return (
    <div className="flex flex-col gap-6 w-full max-w-3xl">
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">General Settings</h1>
        <p className="text-sm text-mid-gray">
          Configure your audio input and output devices.
        </p>
      </div>

      {error && (
        <div className="px-4 py-3 rounded-lg border border-red-500/30 bg-red-500/10 text-sm text-red-700">
          Failed to check BlackHole status: {error}
        </div>
      )}

      {showWarning && (
        <div className="px-4 py-3 rounded-lg border border-yellow-500/30 bg-yellow-500/10 text-sm text-yellow-800">
          <div className="font-medium">BlackHole 2 is not installed</div>
          <div className="mt-1 text-yellow-700/90">
            Audio routing may be unreliable without it. We recommend installing
            BlackHole 2ch, but you can ignore this warning.
          </div>
          <button
            type="button"
            onClick={refresh}
            className="mt-2 text-xs underline text-yellow-800/80 hover:text-yellow-900"
          >
            Check again
          </button>
        </div>
      )}

      {showOutputHint && !showWarning && (
        <div className="px-4 py-3 rounded-lg border border-blue-500/30 bg-blue-500/10 text-sm text-blue-800">
          <div className="font-medium">Output device not selected</div>
          <div className="mt-1 text-blue-700/90">
            Please select an output device (BlackHole recommended) to route processed audio.
          </div>
        </div>
      )}

      <SettingsGroup
        title="Audio I/O"
        description="Pick your microphone input and output device."
      >
        <MicrophoneSelector grouped />
        <MicrophoneVolume />
        <OutputDeviceSelector grouped />
      </SettingsGroup>

      <SettingsGroup
        title="Recording"
        description="Record meetings with processed mic + app audio."
      >
        <RecordingControls />
      </SettingsGroup>
    </div>
  );
};
