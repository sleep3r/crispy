import React, { useEffect, useRef, useState } from "react";
import { useSettings } from "../../hooks/useSettings";

const MODELS = [
  {
    id: "dummy",
    name: "Dummy Model",
    description: "Pass-through (no processing)",
  },
  {
    id: "noisy",
    name: "Noisy Model",
    description: "Adds noise to output",
  },
];

const ModelSelector: React.FC = () => {
  const { getSetting, updateSetting } = useSettings();
  const selectedModel = getSetting("selected_model") || "dummy";
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const currentModel = MODELS.find((m) => m.id === selectedModel) || MODELS[0];

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
    await updateSetting("selected_model", value);
    setIsOpen(false);
  };

  const statusColor =
    selectedModel === "noisy" ? "bg-yellow-400" : "bg-green-500";

  return (
    <div className="relative" ref={dropdownRef}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-2 px-3 py-1.5 bg-mid-gray/5 rounded-md border border-mid-gray/10 hover:bg-mid-gray/10 transition-colors"
      >
        <div className={`w-2 h-2 rounded-full ${statusColor}`} />
        <span className="text-xs font-medium max-w-40 truncate">
          {currentModel.name}
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
          {MODELS.map((model) => (
            <button
              key={model.id}
              type="button"
              onClick={() => handleSelect(model.id)}
              className={`w-full px-3 py-2 text-left hover:bg-mid-gray/10 transition-colors ${
                selectedModel === model.id ? "bg-logo-primary/10" : ""
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

export default ModelSelector;
