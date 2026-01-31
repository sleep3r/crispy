import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingContainer } from "../ui/SettingContainer";

interface LlmSettingsData {
  endpoint: string;
  model: string;
}

export const LlmSettings: React.FC<{ grouped?: boolean }> = ({ grouped = false }) => {
  const [endpoint, setEndpoint] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [model, setModel] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);

  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      const data = await invoke<LlmSettingsData>("get_llm_settings");
      setEndpoint(data.endpoint || "https://api.openai.com/v1");
      setModel(data.model || "gpt-4");
      setApiKey(""); // Don't load key for security
    } catch (err) {
      console.error("Failed to load LLM settings:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await invoke("set_llm_settings", {
        endpoint: endpoint.trim(),
        apiKey: apiKey.trim(),
        model: model.trim(),
      });
      setMessage({ type: "success", text: "Settings saved" });
      setTimeout(() => setMessage(null), 2000);
    } catch (err) {
      setMessage({
        type: "error",
        text: err instanceof Error ? err.message : "Failed to save settings",
      });
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <SettingContainer
        title="LLM Configuration"
        description="Configure your language model settings."
        grouped={grouped}
        layout="stacked"
        descriptionMode="inline"
      >
        <p className="text-sm text-mid-gray">Loading…</p>
      </SettingContainer>
    );
  }

  return (
    <div className="space-y-4">
      <SettingContainer
        title="Endpoint"
        description="OpenAI-compatible API endpoint"
        grouped={grouped}
        layout="stacked"
        descriptionMode="inline"
      >
        <input
          type="url"
          value={endpoint}
          onChange={(e) => setEndpoint(e.target.value)}
          placeholder="https://api.openai.com/v1"
          className="w-full px-3 py-2 rounded-md border border-mid-gray/20 bg-background text-sm focus:outline-none focus:ring-1 focus:ring-mid-gray/30"
        />
      </SettingContainer>

      <SettingContainer
        title="API Key"
        description="Your API key (stored locally)"
        grouped={grouped}
        layout="stacked"
        descriptionMode="inline"
      >
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
          className="w-full px-3 py-2 rounded-md border border-mid-gray/20 bg-background text-sm focus:outline-none focus:ring-1 focus:ring-mid-gray/30"
        />
      </SettingContainer>

      <SettingContainer
        title="Model"
        description="Model name (e.g. gpt-4, gpt-3.5-turbo)"
        grouped={grouped}
        layout="stacked"
        descriptionMode="inline"
      >
        <input
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="gpt-4"
          className="w-full px-3 py-2 rounded-md border border-mid-gray/20 bg-background text-sm focus:outline-none focus:ring-1 focus:ring-mid-gray/30"
        />
      </SettingContainer>

      <div className="px-4 pb-3">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleSave}
            disabled={saving || !endpoint.trim() || !model.trim()}
            className="px-4 py-2 rounded-md bg-mid-gray/15 hover:bg-mid-gray/25 disabled:opacity-50 disabled:pointer-events-none text-sm font-medium transition-colors"
          >
            {saving ? "Saving…" : "Save"}
          </button>
          {message && (
            <span
              className={`text-sm ${
                message.type === "success" ? "text-green-600" : "text-red-600"
              }`}
            >
              {message.text}
            </span>
          )}
        </div>
      </div>
    </div>
  );
};
