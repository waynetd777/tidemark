// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

// Ask: a chat about the Mac, answered by an AI CLI already signed in on it. The first message
// carries what Tidemark knows (the folder on screen, what grew, the performance record); a
// follow-up resumes the CLI's session, and carries it again only if the folder has changed. An Ask
// button elsewhere seeds a question with its own details (an incident, a crash, a chart).

import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { writeHtml } from "@tauri-apps/plugin-clipboard-manager";
import { api } from "./api";
import {
  loadModel,
  modelGroups,
  pickModel,
  providerOf,
  saveModel,
  useAssistant,
  type Provider,
} from "./assistant";
import type { Context } from "./context";
import { mdToHtml } from "./md";

const SUGGESTIONS = [
  "Is anything slowing my Mac down?",
  "Why was my Mac busiest this week?",
  "Where can I free space safely?",
  "What are the biggest folders here for?",
];

interface Message {
  role: "user" | "assistant";
  text: string;
  error?: boolean;
  /** What the question came with: "CPU incident, Wed 14:57". */
  about?: string;
}

/** Details a button hands to Ask with its question: an incident, a crash report, a chart. */
export interface AskDetail {
  /** A few words naming it, shown above the box and on the question. */
  label: string;
  /** What the assistant is told about it. */
  text: string;
}

/** A question handed in from elsewhere, typed into the box to edit or send. */
export interface AskSeed {
  text: string;
  /** Makes a repeat of the same text count. */
  n: number;
  detail?: AskDetail;
}

const uid = () =>
  typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;

export default function AskPanel({
  open,
  onClose,
  context,
  seed,
}: {
  open: boolean;
  onClose: () => void;
  /** What is on screen now; null until the map has something to show. */
  context: () => Context | null;
  /** A question handed in from elsewhere (Help, an Ask button), typed into the box. */
  seed?: AskSeed | null;
}) {
  const [preferred, setModel] = useState(loadModel);
  const assistant = useAssistant();
  // The remembered model if it is still offered, else the first that is.
  const model = pickModel(preferred, assistant.models);
  const [copied, setCopied] = useState<number | null>(null);
  const [chatId, setChatId] = useState(uid);
  const [messages, setMessages] = useState<Message[]>([]);
  const [session, setSession] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [q, setQ] = useState("");
  // The details the next question carries, until it is sent or removed.
  const [detail, setDetail] = useState<AskDetail | null>(null);
  const lastPath = useRef<string | null>(null);
  // The tool the session belongs to: another tool can't resume it, so a switch starts afresh.
  const sessionTool = useRef<Provider | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  // When an input method last finished composing: WebKit ends the composition before the Enter
  // that picks the text arrives, so that Enter is no longer marked as composing.
  const composedAt = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);
  const chatRef = useRef(chatId);
  chatRef.current = chatId;

  // One pair of listeners for the panel's life; a chunk for another chat (one stopped and
  // replaced) is ignored.
  useEffect(() => {
    const chunk = listen<{ chatId: string; text: string }>("ask-chunk", (e) => {
      if (e.payload.chatId !== chatRef.current) return;
      setMessages((m) => {
        const last = m[m.length - 1];
        if (!last || last.role !== "assistant") return m;
        return [...m.slice(0, -1), { ...last, text: last.text + e.payload.text }];
      });
    });
    const done = listen<{
      chatId: string;
      sessionId: string | null;
      text: string;
      error: string | null;
    }>("ask-done", (e) => {
      if (e.payload.chatId !== chatRef.current) return;
      const d = e.payload;
      setMessages((m) => {
        const last = m[m.length - 1];
        if (!last || last.role !== "assistant") return m;
        const fin: Message = d.error
          ? { role: "assistant", text: last.text || d.error, error: true }
          : { ...last, text: last.text || d.text };
        return [...m.slice(0, -1), fin];
      });
      if (d.sessionId) setSession(d.sessionId);
      setBusy(false);
    });
    return () => {
      void chunk.then((un) => un());
      void done.then((un) => un());
    };
  }, []);

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages]);

  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);

  // A new seed is typed into the box as it arrives (derived state, set during render), and the
  // box takes focus with the caret at the end, ready to finish the question.
  const [seen, setSeen] = useState(seed);
  if (seed !== seen) {
    setSeen(seed);
    if (seed) {
      setQ(seed.text);
      setDetail(seed.detail ?? null);
    }
  }
  useEffect(() => {
    if (!seed) return;
    const el = box.current;
    if (el) {
      el.focus();
      window.setTimeout(() => el.setSelectionRange(el.value.length, el.value.length), 0);
    }
  }, [seed]);

  const send = async (question: string) => {
    const text = question.trim();
    if (!text || busy) return;
    const ctx = context();
    const tool = providerOf(model);
    let resume = session;
    if (sessionTool.current !== tool) {
      // The new tool has seen nothing of this chat, so the context goes again.
      resume = null;
      lastPath.current = null;
      setSession(null);
      sessionTool.current = tool;
    }
    let prompt = text;
    // The details go with this question only, in a new chat or a follow-up alike.
    const about = detail ? `About this — ${detail.label}:\n${detail.text}\n\n` : "";
    if (ctx && (!resume || ctx.path !== lastPath.current)) {
      prompt = `${ctx.text}\n\n${about}Question: ${text}`;
      lastPath.current = ctx.path;
    } else if (!ctx && !resume) {
      prompt = `There is no scan on screen yet.\n\n${about}Question: ${text}`;
    } else if (about) {
      prompt = `${about}Question: ${text}`;
    }
    setQ("");
    setDetail(null);
    setBusy(true);
    setMessages((m) => [
      ...m,
      { role: "user", text, about: detail?.label },
      { role: "assistant", text: "" },
    ]);
    try {
      await api.ask(chatId, prompt, model, resume);
    } catch (e) {
      setMessages((m) => [...m.slice(0, -1), { role: "assistant", text: String(e), error: true }]);
      setBusy(false);
    }
  };

  const stop = () => {
    void api.askCancel(chatId);
  };

  const fresh = () => {
    if (busy) void api.askCancel(chatId);
    setChatId(uid());
    setMessages([]);
    setSession(null);
    sessionTool.current = null;
    setBusy(false);
    lastPath.current = null;
    box.current?.focus();
  };

  // Both flavours at once: Markdown for a plain text field, HTML for a rich one.
  const copy = async (i: number, text: string) => {
    try {
      await writeHtml(mdToHtml(text), text);
      setCopied(i);
      window.setTimeout(() => setCopied((c) => (c === i ? null : c)), 1500);
    } catch (e) {
      console.error(e);
    }
  };

  const chooseModel = (id: string) => {
    saveModel(id);
    setModel(id);
  };

  const where = useMemo(() => context()?.path ?? null, [context]);

  return (
    <aside
      className="ask"
      hidden={!open}
      aria-label="Ask about your Mac"
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.stopPropagation()}
      onMouseMove={(e) => e.stopPropagation()}
    >
      <div className="ask-head">
        <b>Ask</b>
        <select
          className="btn small"
          aria-label="Model"
          title="Which model answers"
          value={model}
          onChange={(e) => chooseModel(e.target.value)}
        >
          {modelGroups(assistant.models).map((g) => (
            <optgroup key={g.provider} label={g.name}>
              {g.models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.name}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
        <div className="grow" />
        {messages.length > 0 && (
          <button
            className="btn small"
            onClick={fresh}
            title="Start a new chat about what is on screen"
          >
            New chat
          </button>
        )}
        <button className="btn small" onClick={onClose} aria-label="Close" title="Close (Esc)">
          ✕
        </button>
      </div>
      <div className="ask-scroll" ref={scroller}>
        {messages.length === 0 ? (
          <div className="ask-intro">
            <p>
              Ask about your Mac: why it was slow, what an app was doing, or what is safe to clear
              in {where ? <code>{where}</code> : "the folder on screen"}. The AI sees Tidemark's
              figures, the scan and the crash reports you ask about, not your files, and nothing is
              changed. It runs through the AI tool already signed in on this Mac.
            </p>
            <div className="ask-chips">
              {SUGGESTIONS.map((s) => (
                <button key={s} type="button" className="chip" onClick={() => void send(s)}>
                  {s}
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((m, i) =>
            m.role === "user" ? (
              <div key={i} className="ask-q">
                {m.about && <span className="ask-about">{m.about}</span>}
                {m.text}
              </div>
            ) : m.text ? (
              <div key={i} className="ask-turn">
                <div
                  className={`ask-a${m.error ? " error" : ""}`}
                  dangerouslySetInnerHTML={{ __html: mdToHtml(m.text) }}
                />
                {!m.error && (
                  <button
                    type="button"
                    className="btn small ask-copy"
                    title="Copy this answer as text and as formatted text"
                    onClick={() => void copy(i, m.text)}
                  >
                    {copied === i ? "Copied" : "Copy"}
                  </button>
                )}
              </div>
            ) : (
              <div key={i} className="ask-a working" role="status">
                Thinking
                <span className="dots" aria-hidden="true">
                  <i>.</i>
                  <i>.</i>
                  <i>.</i>
                </span>
              </div>
            ),
          )
        )}
      </div>
      {detail && (
        <div className="ask-detail">
          <span className="ell" title={detail.text}>
            With the details of <b>{detail.label}</b>
          </span>
          <button
            type="button"
            className="link small"
            onClick={() => setDetail(null)}
            title="Ask without these details"
          >
            Remove
          </button>
        </div>
      )}
      <form
        className="ask-input"
        onSubmit={(e) => {
          e.preventDefault();
          void send(q);
        }}
      >
        <textarea
          ref={box}
          rows={2}
          value={q}
          placeholder={messages.length ? "Ask a follow-up…" : "Ask about your Mac…"}
          aria-label="Your question"
          onChange={(e) => setQ(e.target.value)}
          onCompositionEnd={(e) => {
            composedAt.current = e.timeStamp;
          }}
          onKeyDown={(e) => {
            // Not while an input method is composing: there Enter picks the text. WebKit marks
            // that Enter only by key code 229, or not at all once the composition has just ended.
            const composing =
              e.nativeEvent.isComposing ||
              e.keyCode === 229 ||
              e.timeStamp - composedAt.current < 100;
            if (e.key === "Enter" && !e.shiftKey && !composing) {
              e.preventDefault();
              void send(q);
            } else if (e.key === "Escape") {
              e.stopPropagation();
              onClose();
            }
          }}
        />
        {busy ? (
          <button type="button" className="btn stop" onClick={stop} title="Stop the answer">
            Stop
          </button>
        ) : (
          <button type="submit" className="btn" disabled={!q.trim()} title="Send (Enter)">
            Send
          </button>
        )}
      </form>
    </aside>
  );
}
