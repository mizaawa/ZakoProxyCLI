import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { ChangeEvent, FormEvent, KeyboardEvent } from "react";
import {
  Bot,
  Check,
  ChevronDown,
  CircleHelp,
  Copy,
  FolderClosed,
  Globe2,
  KeyRound,
  LoaderCircle,
  MessageSquareText,
  Paperclip,
  Pencil,
  RotateCcw,
  Search,
  Send,
  Settings2,
  SlidersHorizontal,
  Sparkles,
  Square,
  Trash2,
  UserRound,
  X,
} from "lucide-react";
import type { State } from "./api";
import { errorMessage } from "./api";
import { ErrorBox } from "./components";
import type { Notice } from "./components";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import "./playground.css";

type Attachment = { kind: "text" | "image"; name: string; mime_type: string; text?: string; data_url?: string };
type SearchResult = { title: string; url: string; snippet: string };
type Message = { role: "user" | "assistant"; content: string; reasoning?: string; raw?: string; attachments?: Attachment[]; sources?: SearchResult[] };
type SavedSettings = { systemPrompt?: string; temperature?: number; topP?: number; frequencyPenalty?: number; presencePenalty?: number; maxTokens?: number; seed?: string; enabledParams?: { temperature: boolean; topP: boolean; frequencyPenalty: boolean; presencePenalty: boolean; maxTokens: boolean; seed: boolean }; stream?: boolean; searchEnabled?: boolean };
type ReasoningDetail = string | { text?: string; summary?: string };
type ReasoningFields = {
  reasoning_content?: string | null;
  reasoning?: string | null;
  thinking?: string | null;
  reasoning_details?: ReasoningDetail[] | null;
};
type CompletionMessage = ReasoningFields & { content?: string | null };
type Completion = {
  choices?: { message?: CompletionMessage; delta?: CompletionMessage; finish_reason?: string | null }[];
  error?: { message?: string } | string;
};

function reasoningText(value?: ReasoningFields) {
  const direct = [value?.reasoning_content, value?.reasoning, value?.thinking]
    .find((text) => typeof text === "string" && text.trim());
  if (direct) return direct;
  return value?.reasoning_details?.map((detail) =>
    typeof detail === "string" ? detail : detail.text || detail.summary || "",
  ).filter(Boolean).join("\n") || "";
}

function splitThinkContent(value: string, streaming = false) {
  const lower = value.toLowerCase();
  let answer = "";
  let reasoning = "";
  let cursor = 0;
  let insideThink = false;
  while (cursor < value.length) {
    if (insideThink) {
      const end = lower.indexOf("</think>", cursor);
      if (end < 0) {
        reasoning += value.slice(cursor);
        break;
      }
      reasoning += value.slice(cursor, end);
      cursor = end + "</think>".length;
      insideThink = false;
      continue;
    }
    const start = lower.indexOf("<think>", cursor);
    if (start < 0) {
      const remainder = value.slice(cursor);
      let partialTagLength = 0;
      if (streaming) {
        for (let length = Math.min(remainder.length, "<think>".length - 1); length > 0; length--) {
          if (`<think>`.startsWith(remainder.slice(-length).toLowerCase())) {
            partialTagLength = length;
            break;
          }
        }
      }
      answer += remainder.slice(0, remainder.length - partialTagLength);
      break;
    }
    answer += value.slice(cursor, start);
    cursor = start + "<think>".length;
    insideThink = true;
  }
  return { content: answer, reasoning };
}

function joinReasoning(explicit: string, inline: string) {
  if (!explicit) return inline;
  if (!inline || explicit.includes(inline)) return explicit;
  if (inline.includes(explicit)) return inline;
  return `${explicit.trimEnd()}\n\n${inline.trimStart()}`;
}

function responseError(value: Completion, fallback: string) {
  return typeof value.error === "string" ? value.error : value.error?.message || fallback;
}

function attachmentMime(file: File) {
  const extension = file.name.split(".").pop()?.toLowerCase() || "";
  if (["png", "jpg", "jpeg", "webp", "gif"].includes(extension)) {
    return { jpg: "image/jpeg", jpeg: "image/jpeg" }[extension] || `image/${extension}`;
  }
  if (extension === "pdf") return "application/pdf";
  if (extension === "md") return "text/markdown";
  if (extension === "csv") return "text/csv";
  if (extension === "json") return "application/json";
  if (["txt", "log", "js", "jsx", "ts", "tsx", "py", "rs", "html", "css", "xml", "yaml", "yml"].includes(extension)) return "text/plain";
  return file.type;
}

function fileBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error(`无法读取附件“${file.name}”`));
    reader.onload = () => {
      const result = reader.result;
      if (typeof result !== "string" || !result.includes(",")) reject(new Error(`无法编码附件“${file.name}”`));
      else resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.readAsDataURL(file);
  });
}

function Picker({ kind, label, value, options, disabled, onChange }: {
  kind: "group" | "model" | "key";
  label: string;
  value: string;
  options: { value: string; label: string }[];
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const root = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const selected = options.find((option) => option.value === value);
  const visible = options.filter((option) => option.label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  const choose = (next: string) => { onChange(next); setOpen(false); trigger.current?.focus(); };
  const onSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") { setOpen(false); trigger.current?.focus(); return; }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => visible.length ? (current + (event.key === "ArrowDown" ? 1 : -1) + visible.length) % visible.length : 0);
    }
    if (event.key === "Enter" && visible.length) { event.preventDefault(); choose(visible[Math.min(active, visible.length - 1)].value); }
  };
  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("pointerdown", close);
    search.current?.focus();
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  return <div className={`playground-picker ${open ? "open" : ""}`} ref={root}>
    <button ref={trigger} type="button" className="playground-picker-trigger" aria-label={label} aria-haspopup="listbox" aria-controls={listId} aria-expanded={open} disabled={disabled || !options.length} onClick={() => { setQuery(""); setActive(Math.max(0, options.findIndex((option) => option.value === value))); setOpen(!open); }}>
      {kind === "group" ? <FolderClosed size={15} /> : kind === "key" ? <KeyRound size={15} /> : <Bot size={15} />}
      <span>{selected?.label || `选择${label}`}</span>
      <ChevronDown size={14} className="playground-picker-arrow" />
    </button>
    {open && <div className="playground-picker-popover">
      <div className="playground-picker-heading"><strong>选择{label}</strong><span>{visible.length} 个选项</span></div>
      <div className="playground-picker-search"><Search size={15} /><input ref={search} value={query} onChange={(event) => { setQuery(event.target.value); setActive(0); }} onKeyDown={onSearchKeyDown} placeholder={`搜索${label}…`} aria-label={`搜索${label}`} aria-controls={listId} aria-activedescendant={visible.length ? `${listId}-${Math.min(active, visible.length - 1)}` : undefined} /></div>
      <div id={listId} className="playground-picker-options" role="listbox" aria-label={label}>
        {visible.length ? visible.map((option, index) => <button id={`${listId}-${index}`} key={option.value} type="button" role="option" aria-selected={option.value === value} className={index === active ? "is-active" : ""} onPointerMove={() => setActive(index)} onClick={() => choose(option.value)}><span>{option.label}</span>{option.value === value && <Check size={15} />}</button>) : <div className="playground-picker-empty">没有匹配的{label}</div>}
      </div>
    </div>}
  </div>;
}

export function Playground({ state, notify }: { state: State; notify: Notice }) {
  const enabledKeys = state.keys.filter((key) => key.enabled);
  const [keyId, setKeyId] = useState(() => {
    const saved = localStorage.getItem("zako-playground-key");
    return enabledKeys.find((key) => key.id === saved)?.id || enabledKeys[0]?.id || "";
  });
  const selectedKey = enabledKeys.find((key) => key.id === keyId);
  const groups = state.groups.filter((group) => enabledKeys.some((key) => key.group_id === group.id));
  const [models, setModels] = useState<string[]>([]);
  const [model, setModel] = useState("");
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState("");
  const [messages, setMessages] = useState<Message[]>(() => {
    try {
      const saved = JSON.parse(localStorage.getItem("zako-playground-messages") || "[]") as unknown;
      return Array.isArray(saved) ? saved.filter((item): item is Message =>
        !!item && typeof item === "object" && (item.role === "user" || item.role === "assistant") && typeof item.content === "string").slice(-100) : [];
    } catch { return []; }
  });
  const [savedSettings] = useState<SavedSettings>(() => {
    try { return JSON.parse(localStorage.getItem("zako-playground-settings") || "{}") as SavedSettings; }
    catch { return {}; }
  });
  const [draft, setDraft] = useState("");
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  const [attachmentBusy, setAttachmentBusy] = useState(false);
  const [searchEnabled, setSearchEnabled] = useState(savedSettings.searchEnabled === true);
  const [systemPrompt, setSystemPrompt] = useState(typeof savedSettings.systemPrompt === "string" ? savedSettings.systemPrompt : "");
  const [temperature, setTemperature] = useState(typeof savedSettings.temperature === "number" ? savedSettings.temperature : 0.7);
  const [topP, setTopP] = useState(typeof savedSettings.topP === "number" ? savedSettings.topP : 1);
  const [frequencyPenalty, setFrequencyPenalty] = useState(typeof savedSettings.frequencyPenalty === "number" ? savedSettings.frequencyPenalty : 0);
  const [presencePenalty, setPresencePenalty] = useState(typeof savedSettings.presencePenalty === "number" ? savedSettings.presencePenalty : 0);
  const [maxTokens, setMaxTokens] = useState(typeof savedSettings.maxTokens === "number" ? savedSettings.maxTokens : 4096);
  const [seed, setSeed] = useState(typeof savedSettings.seed === "string" ? savedSettings.seed : "");
  const [enabledParams, setEnabledParams] = useState(savedSettings.enabledParams || { temperature: true, topP: true, frequencyPenalty: false, presencePenalty: false, maxTokens: false, seed: false });
  const [stream, setStream] = useState(typeof savedSettings.stream === "boolean" ? savedSettings.stream : true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const chatRef = useRef<HTMLElement>(null);
  const composerRef = useRef<HTMLDivElement>(null);
  const [composerFrame, setComposerFrame] = useState<{ left: number; width: number; height: number; bottom: number } | null>(null);
  useLayoutEffect(() => {
    const measure = () => {
      const chat = chatRef.current?.getBoundingClientRect();
      const composer = composerRef.current?.getBoundingClientRect();
      if (!chat || !composer) return;
      setComposerFrame((previous) => {
        const inset = window.innerWidth <= 650 ? 12 : 20;
        const next = { left: chat.left + inset, width: Math.max(0, chat.width - inset * 2), height: composer.height, bottom: Number.parseFloat(getComputedStyle(composerRef.current!).bottom) || 0 };
        return previous && Math.abs(previous.left - next.left) < 1 && Math.abs(previous.width - next.width) < 1 && Math.abs(previous.height - next.height) < 1 && Math.abs(previous.bottom - next.bottom) < 1 ? previous : next;
      });
    };
    const observer = new ResizeObserver(measure);
    if (chatRef.current) observer.observe(chatRef.current);
    if (composerRef.current) observer.observe(composerRef.current);
    window.addEventListener("resize", measure);
    window.addEventListener("scroll", measure, { passive: true });
    measure();
    return () => { observer.disconnect(); window.removeEventListener("resize", measure); window.removeEventListener("scroll", measure); };
  }, []);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [rawIndex, setRawIndex] = useState<number | null>(null);
  const [groupSearch, setGroupSearch] = useState("");
  const [modelSearch, setModelSearch] = useState("");
  const abortRef = useRef<AbortController | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!enabledKeys.some((key) => key.id === keyId)) setKeyId(enabledKeys[0]?.id || "");
  }, [enabledKeys, keyId]);

  useEffect(() => {
    if (!keyId) {
      setModels([]);
      setModel("");
      return;
    }
    const controller = new AbortController();
    setModelsBusy(true);
    setModelsError("");
    fetch(`/api/playground/models?key_id=${encodeURIComponent(keyId)}`, {
      credentials: "same-origin",
      headers: { "X-Console-Path": window.location.pathname },
      signal: controller.signal,
    })
      .then(async (response) => {
        const body = await response.json();
        if (response.status === 401) window.dispatchEvent(new Event("session-expired"));
        if (!response.ok) throw new Error(responseError(body, `获取模型失败 (${response.status})`));
        return body as { models: string[] };
      })
      .then(({ models: available }) => {
        setModels(available);
        setModel((previous) => available.includes(previous) ? previous : available[0] || "");
      })
      .catch((failure) => {
        if (controller.signal.aborted) return;
        setModels([]);
        setModel("");
        setModelsError(errorMessage(failure));
      })
      .finally(() => { if (!controller.signal.aborted) setModelsBusy(false); });
    return () => controller.abort();
  }, [keyId]);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    const stored = messages.slice(-50).map(({ raw, attachments: files, ...message }) => ({
      ...message,
      attachments: files?.map(({ kind, name, mime_type }) => ({ kind, name, mime_type })),
    }));
    try { localStorage.setItem("zako-playground-messages", JSON.stringify(stored)); }
    catch { /* Storage can be full; the live conversation remains usable. */ }
  }, [messages]);

  useEffect(() => { localStorage.setItem("zako-playground-key", keyId); }, [keyId]);

  useEffect(() => {
    localStorage.setItem("zako-playground-settings", JSON.stringify({ systemPrompt, temperature, topP, frequencyPenalty, presencePenalty, maxTokens, seed, enabledParams, stream, searchEnabled }));
  }, [systemPrompt, temperature, topP, frequencyPenalty, presencePenalty, maxTokens, seed, enabledParams, stream, searchEnabled]);

  useEffect(() => () => abortRef.current?.abort(), []);

  async function addFiles(event: ChangeEvent<HTMLInputElement>) {
    const selected = Array.from(event.target.files || []);
    event.target.value = "";
    if (!selected.length) return;
    let next = [...attachments];
    setAttachmentBusy(true);
    try {
      for (const file of selected) {
        if (next.length >= 5) throw new Error("每条消息最多添加 5 个附件");
        const mime = attachmentMime(file);
        const limit = mime.startsWith("image/") ? 4 * 1024 * 1024 : mime === "application/pdf" ? 2 * 1024 * 1024 : 100_000;
        if (!(["image/png", "image/jpeg", "image/webp", "image/gif", "application/pdf", "text/plain", "text/markdown", "text/csv", "application/json"].includes(mime))) {
          throw new Error(`不支持附件格式：${file.name}`);
        }
        if (file.size > limit) throw new Error(`附件“${file.name}”过大`);
        const response = await fetch("/api/playground/attachment", {
          method: "POST",
          credentials: "same-origin",
          headers: { "Content-Type": "application/json", "X-Console-Path": window.location.pathname },
          body: JSON.stringify({ name: file.name, mime_type: mime, data_base64: await fileBase64(file) }),
        });
        if (response.status === 401) window.dispatchEvent(new Event("session-expired"));
        const value = await response.json() as Attachment & Completion;
        if (!response.ok) throw new Error(responseError(value, `无法处理附件“${file.name}”`));
        next = [...next, value];
        setAttachments(next);
      }
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setAttachmentBusy(false);
    }
  }

  const run = useCallback(async (history: Message[]) => {
    if (!keyId || !model || !history.length || busy) return;
    const requestHistory = history.slice(-50);
    const controller = new AbortController();
    abortRef.current = controller;
    setBusy(true);
    setError("");
    setMessages([...history, { role: "assistant", content: "" }]);
    try {
      let sources: SearchResult[] = [];
      if (searchEnabled && history.at(-1)?.role === "user" && history.at(-1)?.content.trim()) {
        const lookup = await fetch("/api/playground/search", {
          method: "POST",
          credentials: "same-origin",
          headers: { "Content-Type": "application/json", "X-Console-Path": window.location.pathname },
          body: JSON.stringify({ query: history.at(-1)!.content.slice(0, 200) }),
          signal: controller.signal,
        });
        if (lookup.status === 401) window.dispatchEvent(new Event("session-expired"));
        const found = await lookup.json() as { results?: SearchResult[] } & Completion;
        if (!lookup.ok) throw new Error(responseError(found, `联网搜索失败 (${lookup.status})`));
        sources = found.results || [];
        if (!sources.length) throw new Error("未找到联网搜索结果，请调整问题后重试");
        setMessages([...history, { role: "assistant", content: "", sources }]);
      }
      const response = await fetch("/api/playground", {
        method: "POST",
        credentials: "same-origin",
        headers: {
          "Content-Type": "application/json",
          "X-Console-Path": window.location.pathname,
        },
        body: JSON.stringify({
          key_id: keyId,
          model,
          messages: [
            ...(systemPrompt.trim() ? [{ role: "system", content: systemPrompt.trim() }] : []),
            ...requestHistory.map(({ role, content, attachments: files }) => ({
              role,
              content,
              ...(files?.some((file) => file.text || file.data_url)
                ? { attachments: files.filter((file) => file.text || file.data_url) }
                : {}),
            })),
          ],
          ...(sources.length ? { search_results: sources } : {}),
          ...(enabledParams.temperature ? { temperature } : {}),
          ...(enabledParams.topP ? { top_p: topP } : {}),
          ...(enabledParams.frequencyPenalty ? { frequency_penalty: frequencyPenalty } : {}),
          ...(enabledParams.presencePenalty ? { presence_penalty: presencePenalty } : {}),
          ...(enabledParams.maxTokens ? { max_tokens: maxTokens } : {}),
          ...(enabledParams.seed && seed.trim() ? { seed: Number(seed) } : {}),
          stream,
          ...(stream ? { stream_options: { include_usage: true } } : {}),
        }),
        signal: controller.signal,
      });
      if (response.status === 401) window.dispatchEvent(new Event("session-expired"));
      if (!response.ok) {
        const value = await response.json().catch(() => ({})) as Completion;
        throw new Error(responseError(value, `请求失败 (${response.status})`));
      }
      if (stream && response.body && response.headers.get("content-type")?.includes("text/event-stream")) {
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        let responseContent = "";
        let responseReasoning = "";
        let raw = "";
        const consume = (event: string) => {
          const data = event.split(/\r?\n/).filter((line) => line.startsWith("data:")).map((line) => line.slice(5).trimStart()).join("\n");
          if (!data || data === "[DONE]") return;
          const value = JSON.parse(data) as Completion;
          raw += `${data}\n`;
          if (value.error) throw new Error(responseError(value, "生成失败"));
          const delta = value.choices?.[0]?.delta;
          const text = typeof delta?.content === "string" ? delta.content : "";
          const thought = reasoningText(delta);
          if (thought) responseReasoning += thought;
          if (text) responseContent += text;
          if (text || thought) {
            const parts = splitThinkContent(responseContent, true);
            setMessages([...history, {
              role: "assistant",
              content: parts.content,
              reasoning: joinReasoning(responseReasoning, parts.reasoning),
              sources,
            }]);
          }
        };
        while (true) {
          const result = await reader.read();
          if (result.done) break;
          buffer += decoder.decode(result.value, { stream: true });
          const events = buffer.split(/\r?\n\r?\n/);
          buffer = events.pop() || "";
          for (const event of events) consume(event);
        }
        buffer += decoder.decode();
        if (buffer.trim()) consume(buffer);
        const parts = splitThinkContent(responseContent);
        const reasoning = joinReasoning(responseReasoning, parts.reasoning);
        if (!parts.content && !reasoning) throw new Error("模型未返回文本内容");
        setMessages([...history, { role: "assistant", content: parts.content, reasoning, raw, sources }]);
      } else {
        const value = await response.json() as Completion;
        if (value.error) throw new Error(responseError(value, "生成失败"));
        const message = value.choices?.[0]?.message;
        const parts = splitThinkContent(typeof message?.content === "string" ? message.content : "");
        const reasoning = joinReasoning(reasoningText(message), parts.reasoning);
        setMessages([...history, { role: "assistant", content: parts.content, reasoning, raw: JSON.stringify(value, null, 2), sources }]);
      }
    } catch (failure) {
      if (controller.signal.aborted) {
        if (abortRef.current === controller) setMessages((current) => current.at(-1)?.content || current.at(-1)?.reasoning ? current : history);
      } else {
        setMessages((current) => current.at(-1)?.content || current.at(-1)?.reasoning ? current : history);
        setError(errorMessage(failure));
      }
    } finally {
      if (abortRef.current === controller) abortRef.current = null;
      setBusy(false);
    }
  }, [keyId, model, busy, systemPrompt, temperature, topP, frequencyPenalty, presencePenalty, maxTokens, seed, enabledParams, stream, searchEnabled]);

  function submit(event: FormEvent) {
    event.preventDefault();
    const content = draft.trim();
    if ((!content && !attachments.length) || busy || attachmentBusy || !model) return;
    const history = [...messages, { role: "user" as const, content, attachments }];
    setDraft("");
    setAttachments([]);
    void run(history);
  }

  function onDraftKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      event.currentTarget.form?.requestSubmit();
    }
  }

  function reset(confirm = false) {
    if (confirm && messages.length && !window.confirm("清空当前游乐场对话？此操作无法撤销。")) return;
    abortRef.current?.abort();
    abortRef.current = null;
    setMessages([]);
    setDraft("");
    setAttachments([]);
    setError("");
    inputRef.current?.focus();
  }

  function regenerate() {
    const history = messages.at(-1)?.role === "assistant" ? messages.slice(0, -1) : messages;
    if (history.at(-1)?.role === "user") void run(history);
  }

  function editMessage(index: number) {
    const message = messages[index];
    if (message?.role !== "user") return;
    setDraft(message.content);
    setAttachments(message.attachments?.filter((file) => file.text || file.data_url) || []);
    setMessages(messages.slice(0, index));
    inputRef.current?.focus();
  }

  function deleteMessage(index: number) {
    setMessages(messages.filter((_, itemIndex) => itemIndex !== index));
  }

  function toggleParam(name: keyof typeof enabledParams) {
    setEnabledParams((previous) => ({ ...previous, [name]: !previous[name] }));
  }

  async function copyMessage(content: string, index: number) {
    try {
      await navigator.clipboard.writeText(content);
      setCopiedIndex(index);
      window.setTimeout(() => setCopiedIndex(null), 1800);
    } catch (failure) {
      notify(errorMessage(failure), true);
    }
  }

  return (
    <div className="playground-page">
      <div className="playground-layout">
        <aside className={`playground-config ${settingsOpen ? "open" : ""}`} aria-label="游乐场设置">
          <div className="playground-panel-heading">
            <span><SlidersHorizontal size={17} /> 参数设置</span>
            <button type="button" className="playground-config-close" onClick={() => setSettingsOpen(false)} aria-label="关闭参数设置">关闭</button>
          </div>
          <div className="playground-config-body">
            <label className="playground-field">
              <span><FolderClosed size={15} /> 分组</span>
              <input type="search" value={groupSearch} onChange={(event) => setGroupSearch(event.target.value)} placeholder="搜索分组…" aria-label="搜索分组" />
              <Picker kind="group" label="分组" value={selectedKey?.group_id || ""} options={groups.filter((group) => group.name.toLowerCase().includes(groupSearch.toLowerCase()) || group.id === selectedKey?.group_id).map((group) => ({ value: group.id, label: group.name }))} disabled={busy} onChange={(groupId) => { setKeyId(enabledKeys.find((key) => key.group_id === groupId)?.id || ""); reset(); }} />
            </label>
            <label className="playground-field">
              <span><KeyRound size={15} /> API 密钥</span>
              <Picker kind="key" label="API 密钥" value={keyId} options={enabledKeys.filter((key) => key.group_id === selectedKey?.group_id).map((key) => ({ value: key.id, label: key.name }))} disabled={busy} onChange={(id) => { setKeyId(id); reset(); }} />
              <small>使用已启用密钥所属分组的模型。</small>
            </label>
            <label className="playground-field">
              <span><Bot size={15} /> 模型</span>
              <input type="search" value={modelSearch} onChange={(event) => setModelSearch(event.target.value)} placeholder="搜索模型…" aria-label="搜索模型" />
              <span className="playground-select-wrap">
                <Picker kind="model" label="模型" value={model} options={models.filter((name) => name.toLowerCase().includes(modelSearch.toLowerCase()) || name === model).map((name) => ({ value: name, label: name }))} disabled={!models.length || modelsBusy || busy} onChange={setModel} />
              </span>
              {modelsError && <small className="playground-field-error">{modelsError}</small>}
            </label>
            <label className="playground-field">
              <span><MessageSquareText size={15} /> 系统提示词 <em>可选</em></span>
              <textarea value={systemPrompt} onChange={(event) => setSystemPrompt(event.target.value)} placeholder="告诉模型如何回答…" rows={5} disabled={busy} />
            </label>
            <div className="playground-config-divider" />
            <label className="playground-field playground-range-field">
              <span><input type="checkbox" checked={enabledParams.temperature} onChange={() => toggleParam("temperature")} disabled={busy} /> Temperature <output>{temperature.toFixed(1)}</output></span>
              <input type="range" min="0" max="2" step="0.1" value={temperature} onChange={(event) => setTemperature(Number(event.target.value))} disabled={busy || !enabledParams.temperature} />
              <small>较低更稳定，较高更有创意。</small>
            </label>
            <label className="playground-field playground-range-field">
              <span><input type="checkbox" checked={enabledParams.topP} onChange={() => toggleParam("topP")} disabled={busy} /> Top P <output>{topP.toFixed(2)}</output></span>
              <input type="range" min="0" max="1" step="0.05" value={topP} onChange={(event) => setTopP(Number(event.target.value))} disabled={busy || !enabledParams.topP} />
            </label>
            <label className="playground-field playground-range-field">
              <span><input type="checkbox" checked={enabledParams.frequencyPenalty} onChange={() => toggleParam("frequencyPenalty")} disabled={busy} /> Frequency Penalty <output>{frequencyPenalty.toFixed(1)}</output></span>
              <input type="range" min="-2" max="2" step="0.1" value={frequencyPenalty} onChange={(event) => setFrequencyPenalty(Number(event.target.value))} disabled={busy || !enabledParams.frequencyPenalty} />
            </label>
            <label className="playground-field playground-range-field">
              <span><input type="checkbox" checked={enabledParams.presencePenalty} onChange={() => toggleParam("presencePenalty")} disabled={busy} /> Presence Penalty <output>{presencePenalty.toFixed(1)}</output></span>
              <input type="range" min="-2" max="2" step="0.1" value={presencePenalty} onChange={(event) => setPresencePenalty(Number(event.target.value))} disabled={busy || !enabledParams.presencePenalty} />
            </label>
            <label className="playground-field playground-param-number">
              <span><input type="checkbox" checked={enabledParams.maxTokens} onChange={() => toggleParam("maxTokens")} disabled={busy} /> Max Tokens</span>
              <input type="number" min={1} max={32768} value={maxTokens} onChange={(event) => setMaxTokens(Number(event.target.value))} disabled={busy || !enabledParams.maxTokens} />
            </label>
            <label className="playground-field playground-param-number">
              <span><input type="checkbox" checked={enabledParams.seed} onChange={() => toggleParam("seed")} disabled={busy} /> Seed</span>
              <input type="number" placeholder="可选随机种子" value={seed} onChange={(event) => setSeed(event.target.value)} disabled={busy || !enabledParams.seed} />
            </label>
            <label className="playground-switch">
              <span><Settings2 size={15} /> 流式输出</span>
              <input type="checkbox" checked={stream} onChange={(event) => setStream(event.target.checked)} disabled={busy} />
              <i aria-hidden="true" />
            </label>
            <div className="playground-config-hint"><CircleHelp size={15} /><span>请求通过本站中转服务发送，遵循所选密钥的分组策略与调用日志设置。</span></div>
          </div>
        </aside>

        <section className="playground-chat" aria-label="聊天游乐场" ref={chatRef} style={{ paddingBottom: composerFrame ? composerFrame.height + composerFrame.bottom + 20 : 190 }}>
          <div className="playground-messages" ref={scrollRef} aria-live="polite">
            {!messages.length ? (
              <div className="playground-empty">
                <span className="playground-empty-icon"><Sparkles size={30} /></span>
                <h2>开始一场游乐场对话</h2>
                <p>选择分组与模型，提出问题，看看模型如何回答。</p>
                <div className="playground-prompts">
                  {[
                    ["分析数据", "帮我分析以下数据，找出关键趋势："],
                    ["总结文本", "请帮我总结下面这段文本："],
                    ["代码", "请写一段代码，实现以下功能："],
                    ["获取建议", "请针对以下问题给我一些建议："],
                  ].map(([label, prompt]) => <button key={label} type="button" onClick={() => { setDraft(prompt); inputRef.current?.focus(); }}>{label}</button>)}
                </div>
                {!enabledKeys.length && <p className="playground-empty-note">请先在“密钥管理”中创建并启用 API 密钥。</p>}
                {enabledKeys.length > 0 && !models.length && !modelsBusy && <p className="playground-empty-note">当前分组没有启用 Chat Completions 的模型。</p>}
              </div>
            ) : messages.map((message, index) => (
              <article className={`playground-message ${message.role}`} key={index}>
                <span className="playground-message-avatar">{message.role === "user" ? <UserRound size={18} /> : <Bot size={19} />}</span>
                <div className="playground-message-body">
                  <div className="playground-message-meta"><strong>{message.role === "user" ? "你" : "助手"}</strong></div>
                  {message.attachments?.length ? <div className="playground-message-files">{message.attachments.map((file, fileIndex) => <span key={`${file.name}-${fileIndex}`}><Paperclip size={12} />{file.name}</span>)}</div> : null}
                  {message.reasoning && <details className="playground-reasoning" open><summary><Sparkles size={14} /><span>思考过程</span><ChevronDown size={14} /></summary><div className="playground-reasoning-body"><ReactMarkdown remarkPlugins={[remarkGfm]}>{message.reasoning}</ReactMarkdown></div></details>}
                  {message.content ? <div className="playground-message-text">{message.role === "assistant" ? <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown> : message.content}</div> : message.role === "assistant" && busy && index === messages.length - 1 ? <span className="playground-thinking"><i /><i /><i /><span>正在生成…</span></span> : null}
                  {message.sources?.length ? <div className="playground-sources"><strong><Globe2 size={13} /> 搜索来源</strong>{message.sources.map((source, sourceIndex) => <a key={`${source.url}-${sourceIndex}`} href={source.url} target="_blank" rel="noreferrer" title={source.snippet}>{sourceIndex + 1}. {source.title}</a>)}</div> : null}
                  {message.role === "assistant" && message.raw && <div className="playground-raw-toggle"><button type="button" onClick={() => setRawIndex(rawIndex === index ? null : index)}>{rawIndex === index ? "预览" : "原始响应"}</button>{rawIndex === index && <pre>{message.raw}</pre>}</div>}
                  {(message.content || !busy) && <div className="playground-message-actions" aria-label={`${message.role === "user" ? "你的" : "助手"}消息操作`}>
                    {message.content && <button type="button" title="复制" aria-label="复制消息" onClick={() => void copyMessage(message.content, index)}>{copiedIndex === index ? <Check size={15} /> : <Copy size={15} />}</button>}
                    {message.role === "user" && !busy && <button type="button" title="编辑" aria-label="编辑消息" onClick={() => editMessage(index)}><Pencil size={15} /></button>}
                    {!busy && <button type="button" title="删除" aria-label="删除消息" onClick={() => deleteMessage(index)}><Trash2 size={15} /></button>}
                  </div>}
                </div>
              </article>
            ))}
          </div>
        </section>
        <div className="playground-composer-wrap" ref={composerRef} style={composerFrame ? { left: composerFrame.left, width: composerFrame.width } : undefined}>
            <ErrorBox message={error} />
            {error && messages.at(-1)?.role === "user" && <button type="button" className="playground-regenerate" onClick={regenerate}><RotateCcw size={14} /> 重试</button>}
            {messages.length > 0 && !busy && <button type="button" className="playground-regenerate" onClick={regenerate}><RotateCcw size={14} /> 重新生成回复</button>}
            {attachments.length > 0 && <div className="playground-attachment-list">{attachments.map((file, index) => <span key={`${file.name}-${index}`}><Paperclip size={13} />{file.name}<button type="button" aria-label={`移除附件 ${file.name}`} onClick={() => setAttachments((current) => current.filter((_, position) => position !== index))}><X size={13} /></button></span>)}</div>}
            {attachmentBusy && <div className="playground-attachment-status"><LoaderCircle size={13} className="spin" /> 正在处理附件…</div>}
            <form className="playground-composer" onSubmit={submit}>
              <textarea ref={inputRef} value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={onDraftKeyDown} placeholder={model ? "问任何问题…" : "请先选择分组和模型"} rows={1} disabled={!model || busy} aria-label="输入消息" />
              {busy ? <button type="button" className="playground-send stop" onClick={() => abortRef.current?.abort()} aria-label="停止生成" title="停止生成"><Square size={17} fill="currentColor" /></button> : <button type="submit" className="playground-send" disabled={(!draft.trim() && !attachments.length) || !model || attachmentBusy} aria-label="发送消息" title="发送消息"><Send size={18} /></button>}
            </form>
            {searchEnabled && <div className="playground-search-indicator"><Globe2 size={13} /> 联网搜索已开启，发送后会先检索并附上来源。</div>}
            <div className="playground-composer-controls">
              <div className="playground-composer-selects">
                <Picker kind="group" label="分组" value={selectedKey?.group_id || ""} options={groups.map((group) => ({ value: group.id, label: group.name }))} disabled={busy} onChange={(groupId) => { setKeyId(enabledKeys.find((key) => key.group_id === groupId)?.id || ""); reset(); }} />
                <Picker kind="model" label="模型" value={model} options={models.map((name) => ({ value: name, label: name }))} disabled={busy || !models.length} onChange={setModel} />
              </div>
              <div className="playground-composer-actions">
                <input ref={fileRef} className="visually-hidden" type="file" multiple accept=".txt,.md,.csv,.json,.log,.js,.jsx,.ts,.tsx,.py,.rs,.html,.css,.xml,.yaml,.yml,.pdf,image/png,image/jpeg,image/webp,image/gif" onChange={(event) => void addFiles(event)} tabIndex={-1} />
                <button type="button" className="playground-tool" onClick={() => fileRef.current?.click()} disabled={busy || attachmentBusy || attachments.length >= 5} aria-label="添加附件" title="添加附件"><Paperclip size={18} /></button>
                <button type="button" className={`playground-tool ${searchEnabled ? "active" : ""}`} onClick={() => setSearchEnabled(!searchEnabled)} aria-label="联网搜索" aria-pressed={searchEnabled} title={searchEnabled ? "关闭联网搜索" : "开启联网搜索"}><Search size={18} /></button>
                <button type="button" className="playground-tool" onClick={() => setSettingsOpen(!settingsOpen)} aria-label="显示或隐藏参数设置" aria-expanded={settingsOpen} title="参数设置"><SlidersHorizontal size={18} /></button>
                <button type="button" className="playground-tool" onClick={() => reset(true)} disabled={!messages.length} aria-label="清空聊天" title="清空聊天"><Trash2 size={18} /></button>
                <button type="button" className="playground-new-chat" onClick={() => reset(true)} disabled={!messages.length && !draft}><MessageSquareText size={16} />新对话</button>
              </div>
            </div>
        </div>
      </div>
    </div>
  );
}
