import React from "react";
import { Mic, Info, History, Settings, FileVideo } from "lucide-react";
import CrispyTextLogo from "./icons/CrispyTextLogo";
import { GeneralSettings } from "./settings/general/GeneralSettings";
import { AboutSettings } from "./settings/about/AboutSettings";
import { RecordingsHistory } from "./settings/recordings/RecordingsHistory";
import { SettingsPage } from "./settings/SettingsPage";
import { ConvertView } from "./convert/ConvertView";

export type SidebarSection = "general" | "recordings" | "convert" | "settings" | "about";

interface SectionConfig {
  label: string;
  icon: React.ElementType;
  component: React.ComponentType;
}

export const SECTIONS_CONFIG: Record<SidebarSection, SectionConfig> = {
  general: {
    label: "General",
    icon: Mic,
    component: GeneralSettings,
  },
  recordings: {
    label: "Recordings",
    icon: History,
    component: RecordingsHistory,
  },
  convert: {
    label: "Convert",
    icon: FileVideo,
    component: ConvertView,
  },
  settings: {
    label: "Settings",
    icon: Settings,
    component: SettingsPage,
  },
  about: {
    label: "About",
    icon: Info,
    component: AboutSettings,
  },
};

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  return (
    <div className="flex flex-col w-40 h-full border-r border-mid-gray/20 items-center px-2 bg-background">
      <div className="py-4">
        <CrispyTextLogo width={130} />
      </div>
      <div className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20">
        {(Object.entries(SECTIONS_CONFIG) as [SidebarSection, SectionConfig][]).map(
          ([id, config]) => {
            const Icon = config.icon;
            const isActive = activeSection === id;

            return (
              <button
                key={id}
                className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors text-left ${
                  isActive
                    ? "bg-mid-gray/10 text-text font-medium"
                    : "text-mid-gray hover:bg-mid-gray/5 hover:text-text"
                }`}
                onClick={() => onSectionChange(id)}
              >
                <Icon size={20} className="shrink-0" />
                <span className="text-sm truncate">{config.label}</span>
              </button>
            );
          }
        )}
      </div>
    </div>
  );
};
