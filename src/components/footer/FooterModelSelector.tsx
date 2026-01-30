import React, { useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings";
import type { SidebarSection } from "../Sidebar";

const NOISE_MODELS = [
  { id: "dummy", name: "None", description: "Not applied" },
  { id: "noisy", name: "Noisy", description: "Adds noise to output" },
];

const TRANSCRIPTION_MODELS = [
  { id: "none", name: "None", description: "Transcription disabled" },
];

interface FooterModelSelectorProps {
  currentSection: SidebarSection;
}

export const FooterModelSelector: React.FC<FooterModelSelectorProps> = ({
  currentSection,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const isNoise = currentSection === "general";

  const models = isNoise ? NOISE_MODELS : TRANSCRIPTION_MODELS;
  const settingKey = isNoise ? "selected_model" : "selected_transcription_model";
  const selected = getSetting(settingKey) || (isNoise ? "dummy" : "none");
  const current = models.find((m) => m.id === selected) ?? models[0];

  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  const handleSelect = async (value: string) => {
    await updateSetting(settingKey as "selected_model" | "selected_transcription_model", value);
    setIsOpen(false);
  };

  const statusColor =
    isNoise && selected === "noisy" ? "bg-yellow-400" : "bg-green-500";

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 bg-mid-gray/5 rounded-md border border-mid-gray/10 hover:bg-mid-gray/10 transition-colors"
      >
        <div className={`w-2 h-2 rounded-full ${statusColor}`} />
        <span className="text-xs font-medium max-w-40 truncate">
          {current.name}
        </span>
        <svg
          className={`w-3 h-3 transition-transform ${
            isOpen ? "rotate-180" : ""
          }`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute bottom-full left-0 mb-2 w-64 bg-background border border-mid-gray/20 rounded-lg shadow-lg py-1 z-50">
          <div className="px-3 py-1.5 text-xs font-medium text-mid-gray border-b border-mid-gray/20">
            {isNoise ? "Noise suppression" : "Transcription"}
          </div>
          {models.map((model) => (
            <button
              key={model.id}
              type="button"
              onClick={() => handleSelect(model.id)}
              className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                selected === model.id ? "bg-logo-primary/10" : ""
              }`}
            >
              <div className="text-sm font-medium">{model.name}</div>
              <div className="text-xs text-mid-gray">{model.description}</div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
};
