"use client";

import { useLocalParticipant, useParticipants } from "@livekit/components-react";
import { useMutation } from "@tanstack/react-query";
import { MicOff, Send, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { api } from "@/lib/api";

import type { useChat } from "./use-chat";

export type Panel = "people" | "chat";

export function SidePanel({
  panel,
  onClose,
  code,
  isHost,
  chat,
}: {
  panel: Panel;
  onClose: () => void;
  code: string;
  isHost: boolean;
  chat: ReturnType<typeof useChat>;
}) {
  return (
    <aside
      aria-label={panel === "people" ? "Participants" : "Chat"}
      className="flex w-80 shrink-0 flex-col border-l border-white/10"
      style={{ background: "var(--lk-bg2)" }}
    >
      <div className="flex items-center justify-between px-4 py-3">
        <h2 className="font-medium">{panel === "people" ? "People" : "Chat"}</h2>
        <button aria-label="Close panel" className="lk-button" onClick={onClose}>
          <X className="size-4" />
        </button>
      </div>
      {panel === "people" ? <People code={code} isHost={isHost} /> : <Chat chat={chat} />}
    </aside>
  );
}

function People({ code, isHost }: { code: string; isHost: boolean }) {
  const participants = useParticipants();
  const { localParticipant } = useLocalParticipant();

  const remove = useMutation({
    mutationFn: (identity: string) => api.removeParticipant(code, identity),
    onSuccess: () => toast.success("Participant removed"),
    onError: (e) => toast.error(e.message),
  });

  return (
    <ul className="flex-1 space-y-1 overflow-y-auto px-2 pb-3" data-testid="people-list">
      {participants.map((p) => {
        const isLocal = p.identity === localParticipant.identity;
        const pIsHost = p.attributes.role === "host";
        return (
          <li key={p.identity} className="flex items-center gap-3 rounded-md px-2 py-2 hover:bg-white/5">
            <span
              aria-hidden
              className={`grid size-8 shrink-0 place-items-center rounded-full bg-emerald-700 text-sm font-medium ${p.isSpeaking ? "ring-2 ring-emerald-400" : ""}`}
            >
              {(p.name || p.identity).charAt(0).toUpperCase()}
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm">
                {p.name || p.identity}
                {isLocal && <span className="opacity-60"> (you)</span>}
              </p>
              {pIsHost && <p className="text-xs text-emerald-400">Host</p>}
            </div>
            {!p.isMicrophoneEnabled && <MicOff aria-label="Muted" className="size-4 opacity-60" />}
            {isHost && !isLocal && !pIsHost && (
              <button
                className="lk-button text-xs"
                disabled={remove.isPending}
                onClick={() => {
                  if (confirm(`Remove ${p.name || p.identity} from the meeting?`)) remove.mutate(p.identity);
                }}
              >
                Remove
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );
}

function Chat({ chat }: { chat: ReturnType<typeof useChat> }) {
  const [draft, setDraft] = useState("");
  const { localParticipant } = useLocalParticipant();
  const bottom = useRef<HTMLDivElement>(null);

  // Block body: newer browsers return a Promise from scrollIntoView, which React
  // would otherwise treat as the effect's cleanup function and crash.
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [chat.messages.length]);

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const body = draft.trim();
    if (!body) return;
    chat.send.mutate(body, {
      onSuccess: () => setDraft(""),
      onError: (err) => toast.error(err.message),
    });
  }

  return (
    <>
      <div className="flex-1 space-y-3 overflow-y-auto px-4 pb-3" data-testid="chat-messages">
        {chat.loading && <p className="text-sm opacity-60">Loading…</p>}
        {!chat.loading && chat.messages.length === 0 && (
          <p className="text-sm opacity-60">No messages yet. Messages are visible to everyone in the meeting.</p>
        )}
        {chat.messages.map((m) => (
          <div key={m.id}>
            <p className="text-xs opacity-60">
              {m.identity === localParticipant.identity ? "You" : (m.display_name ?? "Former participant")} ·{" "}
              {new Date(m.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
            </p>
            <p className="text-sm break-words whitespace-pre-wrap">{m.body}</p>
          </div>
        ))}
        <div ref={bottom} />
      </div>
      <form onSubmit={submit} className="flex gap-2 border-t border-white/10 p-3">
        <input
          aria-label="Message"
          className="lk-form-control min-w-0 flex-1"
          placeholder="Send a message"
          maxLength={4000}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <button type="submit" aria-label="Send" className="lk-button" disabled={chat.send.isPending || !draft.trim()}>
          <Send className="size-4" />
        </button>
      </form>
    </>
  );
}
