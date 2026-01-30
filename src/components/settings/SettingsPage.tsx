import React from "react";
import { SettingsGroup } from "../ui/SettingsGroup";
import { RecordingsDirectory } from "./RecordingsDirectory";

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
        title="Recording"
        description="Where recordings are stored."
      >
        <RecordingsDirectory grouped />
      </SettingsGroup>
    </div>
  );
};
