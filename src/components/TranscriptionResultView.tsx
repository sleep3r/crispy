import React, { useEffect, useState, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Copy, Check, Send, ArrowDown } from "lucide-react";
import { useTauriListen } from "../hooks/useTauriListen";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/atom-one-dark.css";

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
  { bg: "bg-blue-500/10", text: "text-blue-400", dot: "bg-blue-400" },
  { bg: "bg-emerald-500/10", text: "text-emerald-400", dot: "bg-emerald-400" },
  { bg: "bg-amber-500/10", text: "text-amber-400", dot: "bg-amber-400" },
  { bg: "bg-purple-500/10", text: "text-purple-400", dot: "bg-purple-400" },
  { bg: "bg-rose-500/10", text: "text-rose-400", dot: "bg-rose-400" },
  { bg: "bg-cyan-500/10", text: "text-cyan-400", dot: "bg-cyan-400" },
];

function getSpeakerColor(speaker: string) {
  const match = /\d+/.exec(speaker);
  const idx = match ? (Number.parseInt(match[0], 10) - 1) : 0;
  return SPEAKER_COLORS[idx % SPEAKER_COLORS.length];
}

/** Format seconds as M:SS or H:MM:SS */
function formatTimestamp(totalSeconds: number): string {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

type TranscriptSegment = {
  speaker: string;
  timestamp: number | null;
  text: string;
};

/** Parse transcription text that may contain [Speaker N] or [Speaker N|seconds] markers */
function parseTranscriptionSegments(content: string): TranscriptSegment[] {
  const lines = content.split("\n");
  const segments: TranscriptSegment[] = [];
  let currentSpeaker: string | null = null;
  let currentTimestamp: number | null = null;
  let currentBlock: string[] = [];

  const flushBlock = () => {
    const text = currentBlock.join(" ").trim();
    if (text && currentSpeaker) {
      segments.push({
        speaker: currentSpeaker,
        timestamp: currentTimestamp,
        text,
      });
    } else if (text) {
      segments.push({ speaker: "", timestamp: null, text });
    }
    currentBlock = [];
  };

  for (const line of lines) {
    // Match [Speaker N|seconds] or [Speaker N]
    const speakerMatch = /^\[(.+?)(?:\|(\d+(?:\.\d+)?))?\]\s*$/.exec(line);
    if (speakerMatch) {
      flushBlock();
      currentSpeaker = speakerMatch[1];
      currentTimestamp = speakerMatch[2] ? Number.parseFloat(speakerMatch[2]) : null;
    } else if (line.trim()) {
      currentBlock.push(line.trim());
    }
  }
  flushBlock();

  return segments;
}

/** Render diarized meeting transcript with inline speaker tags and timeline */
function renderMeetingTranscript(content: string): React.ReactNode {
  const segments = parseTranscriptionSegments(content);
  if (segments.length === 0) {
    return <p className="leading-relaxed text-mid-gray/60">(Empty transcription)</p>;
  }

  // Check if any segments have speakers (diarized content)
  const hasSpeakers = segments.some((s) => s.speaker);
  if (!hasSpeakers) {
    return <div className="whitespace-pre-wrap break-words leading-relaxed">{content}</div>;
  }

  return (
    <div className="space-y-0">
      {segments.map((seg, i) => {
        const color = seg.speaker ? getSpeakerColor(seg.speaker) : null;
        const prevSpeaker = i > 0 ? segments[i - 1].speaker : null;
        const isSameSpeaker = seg.speaker === prevSpeaker;

        return (
          <div key={`seg-${i}`} className={isSameSpeaker ? "mt-1" : "mt-4 first:mt-0"}>
            {/* Speaker header - only when speaker changes */}
            {!isSameSpeaker && seg.speaker && color && (
              <div className="flex items-center gap-2 mb-1.5">
                <div className={`w-1.5 h-1.5 rounded-full ${color.dot} shrink-0`} />
                <span className={`text-[11px] font-semibold uppercase tracking-wider ${color.text}`}>
                  {seg.speaker}
                </span>
                {seg.timestamp != null && (
                  <span className="text-[10px] text-mid-gray/40 tabular-nums">
                    {formatTimestamp(seg.timestamp)}
                  </span>
                )}
              </div>
            )}
            {/* Timestamp for same speaker continuation */}
            {isSameSpeaker && seg.timestamp != null && (
              <span className="text-[10px] text-mid-gray/30 tabular-nums mr-1.5">
                {formatTimestamp(seg.timestamp)}
              </span>
            )}
            {/* Text */}
            <p className={`leading-relaxed text-[13px] ${seg.speaker ? "pl-[18px]" : ""}`}>
              {seg.text}
            </p>
          </div>
        );
      })}
    </div>
  );
}

/** Render markdown content (for LLM responses) */
function MarkdownContent({ content }: { readonly content: string }): React.ReactElement {
  return (
    <div className="markdown-content">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
        // Custom styling for markdown elements
        p: ({ children }) => <p className="mb-2 last:mb-0 leading-relaxed">{children}</p>,
        a: ({ children, href }) => (
          <a
            href={href}
            target="_blank"
            rel="noopener noreferrer"
            className="text-logo-primary hover:text-logo-primary/80 underline decoration-logo-primary/30 hover:decoration-logo-primary/60 transition-colors"
          >
            {children}
          </a>
        ),
        code: ({ className, children }) => {
          const isInline = !className;
          if (isInline) {
            return (
              <code className="px-1.5 py-0.5 rounded bg-mid-gray/15 text-[12px] font-mono text-text/90">
                {children}
              </code>
            );
          }
          return (
            <code className={`${className} block p-3 rounded-lg bg-mid-gray/10 text-[12px] overflow-x-auto`}>
              {children}
            </code>
          );
        },
        pre: ({ children }) => <pre className="mb-3 last:mb-0 overflow-hidden rounded-lg">{children}</pre>,
        ul: ({ children }) => <ul className="mb-2 last:mb-0 pl-4 space-y-1 list-disc">{children}</ul>,
        ol: ({ children }) => <ol className="mb-2 last:mb-0 pl-4 space-y-1 list-decimal">{children}</ol>,
        li: ({ children }) => <li className="leading-relaxed">{children}</li>,
        h1: ({ children }) => <h1 className="text-lg font-semibold mb-2 mt-3 first:mt-0">{children}</h1>,
        h2: ({ children }) => <h2 className="text-base font-semibold mb-2 mt-3 first:mt-0">{children}</h2>,
        h3: ({ children }) => <h3 className="text-sm font-semibold mb-1.5 mt-2 first:mt-0">{children}</h3>,
        blockquote: ({ children }) => (
          <blockquote className="border-l-2 border-mid-gray/30 pl-3 py-1 my-2 text-mid-gray/80 italic">
            {children}
          </blockquote>
        ),
        table: ({ children }) => (
          <div className="overflow-x-auto my-2">
            <table className="border-collapse border border-mid-gray/20 text-[12px]">{children}</table>
          </div>
        ),
        th: ({ children }) => (
          <th className="border border-mid-gray/20 px-2 py-1 bg-mid-gray/10 font-semibold text-left">
            {children}
          </th>
        ),
        td: ({ children }) => <td className="border border-mid-gray/20 px-2 py-1">{children}</td>,
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
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
  const lastScrollTopRef = useRef(0);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  const handleScroll = () => {
    const container = messagesContainerRef.current;
    if (!container) return;
    const { scrollTop, scrollHeight, clientHeight } = container;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    
    // Detect scroll direction: if user is actively scrolling up, immediately mark as scrolled up
    const isScrollingUp = scrollTop < lastScrollTopRef.current;
    lastScrollTopRef.current = scrollTop;
    
    // Consider scrolled up if: 1) more than 150px from bottom OR 2) actively scrolling up
    const isScrolledUp = distanceFromBottom > 150 || (isScrollingUp && distanceFromBottom > 10);
    isUserScrolledUpRef.current = isScrolledUp;
    setShowScrollButton(isScrolledUp);
  };

  useEffect(() => {
    if (!isUserScrolledUpRef.current) {
      scrollToBottom();
      // Update lastScrollTop after auto-scroll to prevent false detection
      setTimeout(() => {
        if (messagesContainerRef.current) {
          lastScrollTopRef.current = messagesContainerRef.current.scrollTop;
        }
      }, 100);
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

  /** Check if the first transcription message contains speaker markers (with or without timestamps) */
  const hasSpeakerLabels = useMemo(() => {
    if (messages.length === 0) return false;
    return /\[Speaker \d+(?:\|[\d.]+)?\]/.test(messages[0].content);
  }, [messages]);

  /** Count unique speakers in the transcription */
  const speakerCount = useMemo(() => {
    if (messages.length === 0) return 0;
    const matches = messages[0].content.matchAll(/\[(Speaker \d+)(?:\|[\d.]+)?\]/g);
    const unique = new Set<string>();
    for (const m of matches) unique.add(m[1]);
    return unique.size;
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
          <h1 className="text-base font-semibold tracking-tight">
            {hasSpeakerLabels ? "Meeting Transcript" : "Transcription"}
          </h1>
          {hasSpeakerLabels && speakerCount > 0 && (
            <span className="text-[10px] font-medium text-mid-gray/50 tabular-nums">
              {speakerCount} {speakerCount === 1 ? "speaker" : "speakers"}
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
                  {(() => {
                    // First message with speaker labels - use meeting transcript renderer
                    if (i === 0 && hasSpeakerLabels) {
                      return renderMeetingTranscript(msg.content);
                    }
                    // First message without speaker labels - plain text (transcription)
                    if (i === 0) {
                      return <div className="whitespace-pre-wrap break-words leading-relaxed">{msg.content}</div>;
                    }
                    // LLM chat responses - use Markdown renderer
                    if (msg.content) {
                      return <MarkdownContent content={msg.content} />;
                    }
                    // Streaming indicator
                    if (msg.streaming) {
                      return (
                        <span className="inline-flex gap-1 text-mid-gray">
                          <span className="animate-bounce" style={{ animationDelay: "0ms" }}>.</span>
                          <span className="animate-bounce" style={{ animationDelay: "150ms" }}>.</span>
                          <span className="animate-bounce" style={{ animationDelay: "300ms" }}>.</span>
                        </span>
                      );
                    }
                    return null;
                  })()}
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
              // Update lastScrollTop to prevent false detection after programmatic scroll
              setTimeout(() => {
                if (messagesContainerRef.current) {
                  lastScrollTopRef.current = messagesContainerRef.current.scrollTop;
                }
              }, 100);
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
