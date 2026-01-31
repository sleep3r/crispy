import React, { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Send } from "lucide-react";

const getPathFromQuery = (): string | null => {
  const params = new URLSearchParams(window.location.search);
  return params.get("recording_path");
};

type ChatMessage = {
  role: "user" | "bot";
  name?: string;
  content: string;
};

export const TranscriptionResultView: React.FC = () => {
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [sending, setSending] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const loadTranscription = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const [text, modelId] = await Promise.all([
        invoke<string | null>("get_transcription_result", { recordingPath: path }),
        invoke<string | null>("get_transcription_model", { recordingPath: path }),
      ]);
      let modelName = "Transcription";
      if (modelId && modelId !== "none") {
        const info = await invoke<{ name: string } | null>("get_model_info", { modelId });
        if (info?.name) modelName = info.name;
      }
      const content = text ?? "";
      setMessages([
        {
          role: "bot",
          name: modelName,
          content: content || "(Empty transcription)",
        },
      ]);
      setRecordingPath(path);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load transcription.");
      setMessages([]);
      setRecordingPath(null);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    const unlisten = listen<{ recording_path: string }>("transcription-open", (event) => {
      const p = event?.payload?.recording_path;
      if (p) {
        const url = new URL(window.location.href);
        url.searchParams.set("recording_path", p);
        window.history.replaceState(null, "", url.toString());
        setError(null);
        loadTranscription(p);
      }
    });

    (async () => {
      const fromQuery = getPathFromQuery();
      if (fromQuery) await loadTranscription(fromQuery);
      else {
        setError("No transcription selected.");
        setLoading(false);
      }
    })();

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleSend = async () => {
    const trimmed = inputValue.trim();
    if (!trimmed || !recordingPath || sending) return;
    setInputValue("");
    setMessages((prev) => [...prev, { role: "user", content: trimmed }]);
    setSending(true);
    try {
      const reply = await invoke<string>("ask_transcription_question", {
        recordingPath,
        question: trimmed,
      });
      setMessages((prev) => [...prev, { role: "bot", content: reply }]);
    } catch (err) {
      setMessages((prev) => [
        ...prev,
        { role: "bot", content: err instanceof Error ? err.message : "Error sending question." },
      ]);
    } finally {
      setSending(false);
    }
  };

  if (loading) {
    return (
      <div className="h-screen flex flex-col bg-background text-text p-6">
        <div className="mb-4">
          <h1 className="text-lg font-semibold">Transcription</h1>
        </div>
        <p className="text-mid-gray">Loading…</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-screen flex flex-col bg-background text-text p-6">
        <div className="mb-4">
          <h1 className="text-lg font-semibold">Transcription</h1>
        </div>
        <p className="text-red-500 text-sm">{error}</p>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background text-text overflow-hidden">
      <div className="shrink-0 px-4 py-3 border-b border-mid-gray/20">
        <h1 className="text-lg font-semibold">Transcription</h1>
      </div>

      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.map((msg, i) =>
          msg.role === "bot" ? (
            <div key={i} className="flex flex-col items-start max-w-[85%]">
              {msg.name && (
                <span className="text-xs font-medium text-mid-gray mb-1">{msg.name}</span>
              )}
              <div className="rounded-lg rounded-tl-none bg-mid-gray/10 px-3 py-2 text-sm whitespace-pre-wrap break-words">
                {msg.content}
              </div>
            </div>
          ) : (
            <div key={i} className="flex justify-end">
              <div className="rounded-lg rounded-tr-none bg-slider-fill/15 text-sm px-3 py-2 max-w-[85%] whitespace-pre-wrap break-words">
                {msg.content}
              </div>
            </div>
          )
        )}
        <div ref={messagesEndRef} />
      </div>

      <div className="shrink-0 p-3 border-t border-mid-gray/20 flex gap-2">
        <input
          type="text"
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              handleSend();
            }
          }}
          placeholder="Ask a question about the transcription…"
          className="flex-1 min-w-0 rounded-lg border border-mid-gray/20 bg-background px-3 py-2 text-sm placeholder:text-mid-gray focus:outline-none focus:ring-1 focus:ring-mid-gray/30"
          disabled={sending}
        />
        <button
          type="button"
          onClick={handleSend}
          disabled={sending || !inputValue.trim()}
          className="shrink-0 p-2 rounded-lg bg-mid-gray/15 hover:bg-mid-gray/25 disabled:opacity-50 disabled:pointer-events-none transition-colors"
          title="Send"
        >
          <Send size={18} />
        </button>
      </div>
    </div>
  );
};
