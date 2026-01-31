import React, { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { FooterModelSelector } from "./FooterModelSelector";
import { useTranscriptionModels } from "../../hooks/useTranscriptionModels";
import type { SidebarSection } from "../Sidebar";

interface FooterProps {
  currentSection: SidebarSection;
}

const Footer: React.FC<FooterProps> = ({ currentSection }) => {
  const [version, setVersion] = useState("0.1.0");
  const { downloadSummary } = useTranscriptionModels();
  const [downloadsCollapsed, setDownloadsCollapsed] = useState(false);

  useEffect(() => {
    getVersion().then(setVersion).catch(console.error);
  }, []);

  const showModelSelector =
    currentSection === "general" || currentSection === "recordings";

  return (
    <div className="w-full border-t border-mid-gray/20 bg-background">
      <div className="flex justify-between items-center text-xs px-4 py-2 text-mid-gray">
        <div className="flex items-center gap-4">
          {showModelSelector && (
            <FooterModelSelector currentSection={currentSection} />
          )}
        </div>
        <span>v{version}</span>
      </div>
      {downloadSummary.active && (
        <div className="px-4 pb-2 text-xs text-mid-gray">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={() => setDownloadsCollapsed((prev) => !prev)}
              className="inline-flex items-center gap-2 hover:text-text transition-colors"
            >
              <span className="w-2 h-2 rounded-full bg-logo-primary" />
              {downloadSummary.label}{" "}
              {`${Math.round(downloadSummary.percentage)}%`}
              <svg
                className={`w-3 h-3 transition-transform ${
                  downloadsCollapsed ? "" : "rotate-180"
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
            <span className="tabular-nums ml-auto">
              {downloadSummary.speed.toFixed(1)}MB/s
            </span>
          </div>
          {!downloadsCollapsed && (
            <div className="flex items-center gap-3 mt-2">
              <div className="flex-1 h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
                <div
                  className="h-full bg-logo-primary rounded-full transition-all duration-300"
                  style={{
                    width: `${Math.max(
                      0,
                      Math.min(100, downloadSummary.percentage)
                    )}%`,
                  }}
                />
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default Footer;
