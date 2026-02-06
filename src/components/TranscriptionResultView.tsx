import React, { useEffect, useState, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check, Send, ArrowDown } from "lucide-react";
import { useTauriListen } from "../hooks/useTauriListen";

const getPathFromQuery = (): string | null => {
  const params = new URLSearchParams(globalThis.location.search);
  return params.get("recording_path");
};

type ChatMessage = {
  role: "user" | "bot";
  name?: string;
  content: string;
  chatId?: string;
  streaming?: boolean;
};

/** Speaker label colors - muted, accessible palette */
const SPEAKER_COLORS = [
  { bg: "bg-blue-500/10", text: "text-blue-400", border: "border-blue-500/30" },
  { bg: "bg-emerald-500/10", text: "text-emerald-400", border: "border-emerald-500/30" },
  { bg: "bg-amber-500/10", text: "text-amber-400", border: "border-amber-500/30" },
  { bg: "bg-purple-500/10", text: "text-purple-400", border: "border-purple-500/30" },
  { bg: "bg-rose-500/10", text: "text-rose-400", border: "border-rose-500/30" },
  { bg: "bg-cyan-500/10", text: "text-cyan-400", border: "border-cyan-500/30" },
];

function getSpeakerColor(speaker: string) {
  // Extract number from "Speaker N" or use hash
  const match = /\d+/.exec(speaker);
  const idx = match ? (Number.parseInt(match[0], 10) - 1) : 0;
  return SPEAKER_COLORS[idx % SPEAKER_COLORS.length];
}

/** Parse transcription text that may contain [Speaker N] markers */
function parseTranscriptionContent(content: string): React.ReactNode {
  const lines = content.split("\n");
  const elements: React.ReactNode[] = [];
  let currentSpeaker: string | null = null;
  let currentBlock: string[] = [];

  const flushBlock = () => {
    if (currentBlock.length === 0) return;
    const text = currentBlock.join("\n").trim();
    if (!text) {
      currentBlock = [];
      return;
    }
    if (currentSpeaker) {
      const color = getSpeakerColor(currentSpeaker);
      elements.push(
        <div key={`block-${elements.length}`} className={`border-l-2 ${color.border} pl-3 py-1.5 my-2`}>
          <span className={`text-[11px] font-semibold uppercase tracking-wider ${color.text} ${color.bg} rounded px-1.5 py-0.5`}>
            {currentSpeaker}
          </span>
          <p className="mt-1 leading-relaxed">{text}</p>
        </div>
      );
    } else {
      elements.push(
        <p key={`block-${elements.length}`} className="leading-relaxed my-1">{text}</p>
      );
    }
    currentBlock = [];
  };

  for (const line of lines) {
    const speakerMatch = /^\[(.+?)\]\s*$/.exec(line);
    if (speakerMatch) {
      flushBlock();
      currentSpeaker = speakerMatch[1];
    } else {
      currentBlock.push(line);
    }
  }
  flushBlock();

  return <>{elements}</>;
}

export const TranscriptionResultView: React.FC = () => {
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inputValue, setInputValue] = useState("");
  const [sending, setSending] = useState(false);
  const [llmModelName, setLlmModelName] = useState<string>("Assistant");
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [showScrollButton, setShowScrollButton] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const messagesContainerRef = useRef<HTMLDivElement>(null);
  const recordingPathRef = useRef<string | null>(null);
  const isUserScrolledUpRef = useRef(false);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  const handleScroll = () => {
    const container = messagesContainerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    const isScrolledUp = distanceFromBottom > 50;
    isUserScrolledUpRef.current = isScrolledUp;
    setShowScrollButton(isScrolledUp);
  };

  useEffect(() => {
    if (!isUserScrolledUpRef.current) {
      scrollToBottom();
    }
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

  // Setup event listeners with proper lifecycle management
  useTauriListen<{ recording_path: string }>("transcription-open", (event) => {
    const p = event?.payload?.recording_path;
    if (p) {
      const url = new URL(globalThis.location.href);
      url.searchParams.set("recording_path", p);
      globalThis.history.replaceState(null, "", url.toString());
      setError(null);
      loadTranscription(p);
    }
  });

  useTauriListen<{ chat_id: string; delta: string }>(
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

  useTauriListen<{ chat_id: string }>("transcription-chat-done", (event) => {
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

  useTauriListen<{ chat_id: string; delta: string }>(
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

  // Initial load from query params
  useEffect(() => {
    (async () => {
      const fromQuery = getPathFromQuery();
      if (fromQuery) await loadTranscription(fromQuery);
      else {
        setError("No transcription selected.");
        setLoading(false);
      }
    })();
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
    isUserScrolledUpRef.current = false;
    setShowScrollButton(false);

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

  /** Check if the first transcription message contains speaker markers */
  const hasSpeakerLabels = useMemo(() => {
    if (messages.length === 0) return false;
    return /\[Speaker \d+\]/.test(messages[0].content);
  }, [messages]);

  if (loading) {
    return (
      <div className="h-screen flex flex-col bg-background text-text">
        <div className="shrink-0 px-5 py-4 border-b border-mid-gray/10">
          <h1 className="text-base font-semibold tracking-tight">Transcription</h1>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <p className="text-mid-gray text-sm">Loading...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-screen flex flex-col bg-background text-text">
        <div className="shrink-0 px-5 py-4 border-b border-mid-gray/10">
          <h1 className="text-base font-semibold tracking-tight">Transcription</h1>
        </div>
        <div className="flex-1 flex items-center justify-center px-6">
          <p className="text-red-400 text-sm text-center">{error}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background text-text overflow-hidden relative">
      {/* Header */}
      <div className="shrink-0 px-5 py-3.5 border-b border-mid-gray/10 bg-background/80 backdrop-blur-sm">
        <div className="flex items-center justify-between">
          <h1 className="text-base font-semibold tracking-tight">Transcription</h1>
          {hasSpeakerLabels && (
            <span className="text-[10px] font-medium text-mid-gray/60 uppercase tracking-widest">
              Diarized
            </span>
          )}
        </div>
      </div>

      {/* Messages */}
      <div
        ref={messagesContainerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-5 py-4 space-y-5"
      >
        {messages.map((msg, i) =>
          msg.role === "bot" ? (
            <div key={msg.chatId ?? `bot-${i}`} className="flex flex-col items-start">
              {/* Speaker name label */}
              {msg.name && (
                <div className="flex items-center gap-2 mb-2">
                  <div className="w-1.5 h-1.5 rounded-full bg-logo-primary/60" />
                  <span className="text-[11px] font-semibold text-mid-gray/70 uppercase tracking-wider">
                    {msg.name}
                  </span>
                </div>
              )}
              {/* Message bubble */}
              <div className="relative w-full rounded-xl bg-mid-gray/[0.06] border border-mid-gray/[0.08] px-4 py-3 text-[13px] group">
                <div className={msg.name ? "pr-8" : ""}>
                  {i === 0 && hasSpeakerLabels
                    ? parseTranscriptionContent(msg.content)
                    : (
                      <div className="whitespace-pre-wrap break-words leading-relaxed">
                        {msg.content || (msg.streaming && (
                          <span className="inline-flex gap-1 text-mid-gray">
                            <span className="animate-bounce" style={{ animationDelay: "0ms" }}>.</span>
                            <span className="animate-bounce" style={{ animationDelay: "150ms" }}>.</span>
                            <span className="animate-bounce" style={{ animationDelay: "300ms" }}>.</span>
                          </span>
                        ))}
                      </div>
                    )
                  }
                </div>
                {msg.name && (
                  <button
                    type="button"
                    onClick={() => handleCopy(msg.content, i)}
                    className="absolute top-3 right-3 rounded-md p-1 text-mid-gray/30 hover:text-text hover:bg-mid-gray/10 transition-all opacity-0 group-hover:opacity-100"
                    title="Copy"
                  >
                    {copiedIndex === i ? <Check size={14} /> : <Copy size={14} />}
                  </button>
                )}
              </div>
            </div>
          ) : (
            <div key={msg.chatId ?? `user-${i}`} className="flex justify-end">
              <div className="rounded-xl rounded-br-sm bg-logo-primary/10 border border-logo-primary/15 text-[13px] px-4 py-3 max-w-[85%] whitespace-pre-wrap break-words leading-relaxed">
                {msg.content}
              </div>
            </div>
          )
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Scroll to bottom */}
      {showScrollButton && (
        <div className="absolute bottom-[72px] right-5 z-10">
          <button
            type="button"
            onClick={() => {
              isUserScrolledUpRef.current = false;
              setShowScrollButton(false);
              scrollToBottom();
            }}
            className="p-2 rounded-full bg-background border border-mid-gray/20 shadow-md hover:bg-mid-gray/5 transition-colors"
            title="Scroll to bottom"
          >
            <ArrowDown size={16} className="text-mid-gray/60" />
          </button>
        </div>
      )}

      {/* Input area */}
      <div className="shrink-0 px-4 py-3 border-t border-mid-gray/10 bg-background/80 backdrop-blur-sm">
        <div className="flex gap-2 items-end">
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
            placeholder="Ask about this transcription..."
            className="flex-1 min-w-0 rounded-xl border border-mid-gray/15 bg-mid-gray/[0.04] px-4 py-2.5 text-[13px] placeholder:text-mid-gray/40 focus:outline-none focus:ring-1 focus:ring-logo-primary/30 focus:border-logo-primary/20 transition-colors"
            disabled={sending}
          />
          <button
            type="button"
            onClick={handleSend}
            disabled={sending || !inputValue.trim()}
            className="shrink-0 p-2.5 rounded-xl bg-logo-primary/10 hover:bg-logo-primary/20 text-logo-primary disabled:opacity-30 disabled:pointer-events-none transition-colors"
            title="Send"
          >
            <Send size={16} />
          </button>
        </div>
      </div>
    </div>
  );
};
