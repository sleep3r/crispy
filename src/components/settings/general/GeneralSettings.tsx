import React from "react";
import { SettingsGroup } from "../../ui/SettingsGroup";
import {
  MicrophoneSelector,
  OutputDeviceSelector,
  MicrophoneVolume,
} from "../";
import { useBlackHoleStatus } from "../../../hooks/useBlackHoleStatus";

export const GeneralSettings: React.FC = () => {
  const { status, isLoading, error, refresh } = useBlackHoleStatus();
  const isBlackHoleInstalled = status?.installed ?? false;
  const isBlocked = !isLoading && !isBlackHoleInstalled;

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

      {isBlocked && (
        <div className="px-4 py-3 rounded-lg border border-yellow-500/30 bg-yellow-500/10 text-sm text-yellow-800">
          <div className="font-medium">BlackHole 2 required</div>
          <div className="mt-1 text-yellow-700/90">
            Install BlackHole 2ch to continue. Without it, the audio routing
            won’t work.
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

      <SettingsGroup
        title="Audio I/O"
        description="Pick your microphone input and output device."
      >
        <MicrophoneSelector grouped disabled={isBlocked} />
        <MicrophoneVolume disabled={isBlocked} />
        <OutputDeviceSelector grouped disabled={isBlocked} />
      </SettingsGroup>
    </div>
  );
};
