import React from "react";
import { SettingsGroup } from "../ui/SettingsGroup";
import { RecordingsDirectory } from "./RecordingsDirectory";
import { LlmSettings } from "./LlmSettings";
import { AutostartToggle } from "./AutostartToggle";

export const SettingsPage: React.FC = () => {
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">Settings</h1>
        <p className="text-sm text-mid-gray">
          Paths and other app preferences.
        </p>
      </div>

      <SettingsGroup
        title="Application"
        description="App startup and behavior preferences."
      >
        <AutostartToggle grouped />
      </SettingsGroup>

      <SettingsGroup
        title="Recording"
        description="Where recordings are stored."
      >
        <RecordingsDirectory grouped />
      </SettingsGroup>

      <SettingsGroup
        title="LLM Chat"
        description="Configure language model for transcription chat."
      >
        <LlmSettings grouped />
      </SettingsGroup>
    </div>
  );
};
