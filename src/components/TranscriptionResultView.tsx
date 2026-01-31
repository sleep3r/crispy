import React, { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Copy, Check, Send } from "lucide-react";

const getPathFromQuery = (): string | null => {
  const params = new URLSearchParams(globalThis.location.search);
  return params.get("recording_path");
};

type ChatMessage = {
  role: "user" | "bot";
  name?: string;
  content: string;
  chatId?: string; // For tracking streaming responses
  streaming?: boolean; // If bot message is still streaming
};

export const TranscriptionResultView: React.FC = () => {
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [sending, setSending] = useState(false);
  const [llmModelName, setLlmModelName] = useState<string>("Assistant");
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const recordingPathRef = useRef<string | null>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  useEffect(() => {
    recordingPathRef.current = recordingPath;
  }, [recordingPath]);

  const loadTranscription = async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const [text, modelId, history, llm] = await Promise.all([
        invoke<string | null>("get_transcription_result", { recordingPath: path }),
        invoke<string | null>("get_transcription_model", { recordingPath: path }),
        invoke<{ role: string; content: string }[]>("get_transcription_chat_history", {
          recordingPath: path,
        }),
        invoke<{ model: string }>("get_llm_settings"),
      ]);
      let modelName = "Transcription";
      if (modelId && modelId !== "none") {
        const info = await invoke<{ name: string } | null>("get_model_info", { modelId });
        if (info?.name) modelName = info.name;
      }
      const assistantName = llm?.model?.trim() || "Assistant";
      setLlmModelName(assistantName);
      const content = text ?? "";
      const historyMessages: ChatMessage[] = (history || []).map((m) => ({
        role: m.role === "user" ? "user" : "bot",
        content: m.content,
        name: m.role === "assistant" ? assistantName : undefined,
      }));
      setMessages([
        {
          role: "bot",
          name: modelName,
          content: content || "(Empty transcription)",
        },
        ...historyMessages,
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
    const unlistenOpen = listen<{ recording_path: string }>("transcription-open", (event) => {
      const p = event?.payload?.recording_path;
      if (p) {
        const url = new URL(globalThis.location.href);
        url.searchParams.set("recording_path", p);
        globalThis.history.replaceState(null, "", url.toString());
        setError(null);
        loadTranscription(p);
      }
    });

    const unlistenStream = listen<{ chat_id: string; delta: string }>(
      "transcription-chat-stream",
      (event) => {
        const { chat_id, delta } = event.payload;
        setMessages((prev) => {
          const lastIndex = prev.findIndex(
            (m) => m.role === "bot" && m.chatId === chat_id && m.streaming
          );
          if (lastIndex === -1) return prev;
          const updated = [...prev];
          updated[lastIndex] = {
            ...updated[lastIndex],
            content: updated[lastIndex].content + delta,
          };
          return updated;
        });
      }
    );

    const unlistenDone = listen<{ chat_id: string }>("transcription-chat-done", (event) => {
      const { chat_id } = event.payload;
      setMessages((prev) => {
        const next = prev.map((m) =>
          m.chatId === chat_id ? { ...m, streaming: false } : m
        );
        persistChatHistory(next);
        return next;
      });
      setSending(false);
    });

    const unlistenError = listen<{ chat_id: string; delta: string }>(
      "transcription-chat-error",
      (event) => {
        const { chat_id, delta } = event.payload;
        setMessages((prev) => {
          const lastIndex = prev.findIndex((m) => m.chatId === chat_id && m.streaming);
          if (lastIndex === -1) return prev;
          const updated = [...prev];
          updated[lastIndex] = {
            ...updated[lastIndex],
            content: delta,
            streaming: false,
          };
          persistChatHistory(updated);
          return updated;
        });
        setSending(false);
      }
    );

    (async () => {
      const fromQuery = getPathFromQuery();
      if (fromQuery) await loadTranscription(fromQuery);
      else {
        setError("No transcription selected.");
        setLoading(false);
      }
    })();

    return () => {
      unlistenOpen.then((fn) => fn());
      unlistenStream.then((fn) => fn());
      unlistenDone.then((fn) => fn());
      unlistenError.then((fn) => fn());
    };
  }, []);

  const handleSend = async () => {
    const trimmed = inputValue.trim();
    if (!trimmed || !recordingPath || sending) return;
    setInputValue("");
    setSending(true);

    const userMsg: ChatMessage = { role: "user", content: trimmed };
    const chatId = `chat_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`;
    const botMsg: ChatMessage = {
      role: "bot",
      content: "",
      name: llmModelName,
      chatId,
      streaming: true,
    };

    setMessages((prev) => [...prev, userMsg, botMsg]);

    try {
      const chatHistory = [
        ...messages.filter((m) => !m.streaming),
        userMsg,
      ].map((m) => ({
        role: m.role === "user" ? "user" : "assistant",
        content: m.content,
      }));

      await invoke("stream_transcription_chat", {
        recordingPath,
        messages: chatHistory,
        chatId,
      });
    } catch (err) {
      setMessages((prev) =>
        prev.map((m) =>
          m.chatId === chatId
            ? {
                ...m,
                content: err instanceof Error ? err.message : "Error sending question.",
                streaming: false,
              }
            : m
        )
      );
      setSending(false);
    }
  };

  const handleCopy = async (content: string, index: number) => {
    if (!content) return;
    try {
      if (!navigator?.clipboard?.writeText) {
        console.error("Clipboard API is not available in this context.");
        return;
      }
      await navigator.clipboard.writeText(content);
      setCopiedIndex(index);
      setTimeout(() => setCopiedIndex((current) => (current === index ? null : current)), 1500);
    } catch (err) {
      console.error("Failed to copy message:", err);
    }
  };

  const persistChatHistory = (nextMessages: ChatMessage[]) => {
    const path = recordingPathRef.current;
    if (!path) return;
    const payload = nextMessages
      .filter((_, index) => index > 0)
      .filter((m) => !m.streaming)
      .map((m) => ({
        role: m.role === "user" ? "user" : "assistant",
        content: m.content,
      }));
    invoke("set_transcription_chat_history", { recordingPath: path, messages: payload }).catch(
      console.error
    );
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
            <div key={msg.chatId ?? `bot-${i}`} className="flex flex-col items-start max-w-[85%]">
              {msg.name && (
                <span className="text-xs font-medium text-mid-gray mb-1">{msg.name}</span>
              )}
              <div className="relative rounded-lg rounded-tl-none bg-mid-gray/10 px-3 py-2 text-sm whitespace-pre-wrap break-words group">
                <div className={msg.name ? "pr-7" : ""}>
                  {msg.content || (msg.streaming && "...")}
                </div>
                {msg.name && (
                  <button
                    type="button"
                    onClick={() => handleCopy(msg.content, i)}
                    className="absolute top-1.5 right-1.5 rounded-md p-1 text-mid-gray/50 hover:text-text hover:bg-mid-gray/20 transition-all"
                    title="Copy message"
                  >
                    {copiedIndex === i ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div key={msg.chatId ?? `user-${i}`} className="flex justify-end">
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
