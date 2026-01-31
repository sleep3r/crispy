import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";

const openUrl = async (url: string) => {
  try {
    await invoke("open_url", { url });
  } catch (error) {
    console.error("Failed to open URL:", error);
  }
};

export const AboutSettings: React.FC = () => {
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title="About">
        <SettingContainer
          title="Support & development"
          description="If you want to support the project, you can do it here."
          grouped={true}
        >
          <button
            type="button"
            onClick={() => openUrl("https://stripe.com")}
            className="inline-flex items-center px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
          >
            Open Stripe
          </button>
        </SettingContainer>

        <SettingContainer
          title="Source Code"
          description="View the source code on GitHub."
          grouped={true}
        >
          <button
            type="button"
            onClick={() => openUrl("https://github.com/sleep3r/crispy")}
            className="inline-flex items-center px-3 py-1.5 text-sm rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors"
          >
            Open GitHub
          </button>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title="Acknowledgements">
        <SettingContainer
          title="Thanks"
          description="In gratitude to the open-source community."
          grouped={true}
          layout="stacked"
        >
          <div className="text-sm text-mid-gray">Coming soon.</div>
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
