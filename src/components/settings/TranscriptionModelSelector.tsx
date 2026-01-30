import React, { useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings";
import { SettingContainer } from "../ui/SettingContainer";

const TRANSCRIPTION_MODELS = [
  {
    id: "none",
    name: "None",
    description: "Transcription disabled",
  },
  // Placeholder for future: whisper, etc.
];

export const TranscriptionModelSelector: React.FC<{ grouped?: boolean }> = ({
  grouped = false,
}) => {
  const { getSetting, updateSetting } = useSettings();
  const selected = getSetting("selected_transcription_model") || "none";
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const current = TRANSCRIPTION_MODELS.find((m) => m.id === selected) ?? TRANSCRIPTION_MODELS[0];

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
    await updateSetting("selected_transcription_model", value);
    setIsOpen(false);
  };

  const content = (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 w-full rounded-md border border-mid-gray/20 bg-background hover:bg-mid-gray/5 transition-colors text-left"
      >
        <span className="text-sm flex-1 truncate">{current.name}</span>
        <svg
          className={`w-4 h-4 shrink-0 text-mid-gray transition-transform ${
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
        <div className="absolute top-full left-0 right-0 mt-1 bg-background border border-mid-gray/20 rounded-lg shadow-lg py-1 z-50">
          {TRANSCRIPTION_MODELS.map((model) => (
            <button
              key={model.id}
              type="button"
              onClick={() => handleSelect(model.id)}
              className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                selected === model.id ? "bg-mid-gray/10" : ""
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

  if (grouped) {
    return (
      <SettingContainer
        title="Transcription model"
        description="Model used to transcribe recordings. More options coming soon."
        grouped
      >
        {content}
      </SettingContainer>
    );
  }

  return (
    <SettingContainer
      title="Transcription model"
      description="Model used to transcribe recordings. More options coming soon."
    >
      {content}
    </SettingContainer>
  );
};
