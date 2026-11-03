"use client";

import { useLocalParticipant, useParticipants } from "@livekit/components-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Hand, MicOff, Send, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { type Meeting, api } from "@/lib/api";

import type { useChat } from "./use-chat";

export type Panel = "people" | "chat";

export function SidePanel({
  panel,
  onClose,
  code,
  participantToken,
  isHost,
  meeting,
  chat,
}: {
  panel: Panel;
  onClose: () => void;
  code: string;
  participantToken: string;
  isHost: boolean;
  meeting: Meeting;
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
      {panel === "people" ? (
        <>
          {isHost && <HostSettings code={code} participantToken={participantToken} meeting={meeting} />}
          <People code={code} participantToken={participantToken} isHost={isHost} />
        </>
      ) : (
        <Chat chat={chat} />
      )}
    </aside>
  );
}

function HostSettings({ code, participantToken, meeting: initial }: { code: string; participantToken: string; meeting: Meeting }) {
  const qc = useQueryClient();
  // Shared with the meeting page query, so reopening the panel shows current settings.
  const { data: meeting = initial } = useQuery({
    queryKey: ["meeting", code],
    queryFn: () => api.meeting(code),
    initialData: initial,
  });
  const settings = { hasPassword: meeting.has_password };
  // Local so the checkbox flips on click (a controlled input would snap back until the save returns).
  const [lobby, setLobby] = useState(meeting.lobby_enabled);
  const [password, setPassword] = useState("");

  const update = useMutation({
    mutationFn: (body: { lobby_enabled?: boolean; password?: string | null }) =>
      api.updateSettings(code, participantToken, body),
    onSuccess: (m) => {
      qc.setQueryData(["meeting", code], m);
      setPassword("");
      toast.success("Meeting settings saved");
    },
    onError: (e) => {
      setLobby(meeting.lobby_enabled); // roll back the optimistic toggle
      toast.error(e.message);
    },
  });
  const muteAll = useMutation({
    mutationFn: () => api.muteAll(code, participantToken),
    onSuccess: () => toast.success("Everyone else is muted"),
    onError: (e) => toast.error(e.message),
  });

  return (
    <section aria-label="Host controls" className="space-y-3 border-b border-white/10 px-4 pb-4">
      <button className="lk-button w-full justify-center" disabled={muteAll.isPending} onClick={() => muteAll.mutate()}>
        <MicOff className="size-4" /> Mute everyone else
      </button>
      <label className="flex items-center justify-between gap-2 text-sm">
        Lobby: approve people before they join
        <input
          type="checkbox"
          className="size-4 accent-emerald-500"
          checked={lobby}
          disabled={update.isPending}
          onChange={(e) => {
            setLobby(e.target.checked);
            update.mutate({ lobby_enabled: e.target.checked });
          }}
        />
      </label>
      <form
        className="space-y-1"
        onSubmit={(e) => {
          e.preventDefault();
          update.mutate({ password });
        }}
      >
        <label htmlFor="host-password" className="text-sm">
          Meeting password {settings.hasPassword ? "(set)" : "(none)"}
        </label>
        <div className="flex gap-2">
          <input
            id="host-password"
            type="password"
            minLength={6}
            maxLength={128}
            placeholder={settings.hasPassword ? "New password" : "Set a password"}
            className="lk-form-control min-w-0 flex-1"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          <button type="submit" className="lk-button text-xs" disabled={update.isPending || password.length < 6}>
            Save
          </button>
        </div>
        {settings.hasPassword && (
          <button
            type="button"
            className="text-xs underline opacity-70"
            disabled={update.isPending}
            onClick={() => update.mutate({ password: null })}
          >
            Remove password
          </button>
        )}
      </form>
    </section>
  );
}

function People({ code, participantToken, isHost }: { code: string; participantToken: string; isHost: boolean }) {
  const participants = useParticipants();
  const { localParticipant } = useLocalParticipant();

  const remove = useMutation({
    mutationFn: (identity: string) => api.removeParticipant(code, participantToken, identity),
    onSuccess: () => toast.success("Participant removed"),
    onError: (e) => toast.error(e.message),
  });
  const mute = useMutation({
    mutationFn: (identity: string) => api.mute(code, participantToken, identity),
    onError: (e) => toast.error(e.message),
  });

  // Raised hands first, in the order LiveKit reports them.
  const sorted = [...participants].sort((a, b) => Number(b.attributes.hand === "1") - Number(a.attributes.hand === "1"));

  return (
    <ul className="flex-1 space-y-1 overflow-y-auto px-2 py-3" data-testid="people-list">
      {sorted.map((p) => {
        const isLocal = p.identity === localParticipant.identity;
        const pIsHost = p.attributes.role === "host";
        const handUp = p.attributes.hand === "1";
        const name = p.name || p.identity;
        return (
          <li key={p.identity} className="flex items-center gap-2 rounded-md px-2 py-2 hover:bg-white/5">
            <span
              aria-hidden
              className={`grid size-8 shrink-0 place-items-center rounded-full bg-emerald-700 text-sm font-medium ${p.isSpeaking ? "ring-2 ring-emerald-400" : ""}`}
            >
              {name.charAt(0).toUpperCase()}
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm">
                {name}
                {isLocal && <span className="opacity-60"> (you)</span>}
              </p>
              {pIsHost && <p className="text-xs text-emerald-400">Host</p>}
            </div>
            {handUp && <Hand aria-label="Hand raised" className="size-4 text-amber-400" />}
            {!p.isMicrophoneEnabled && <MicOff aria-label="Muted" className="size-4 opacity-60" />}
            {isHost && !isLocal && !pIsHost && (
              <>
                {p.isMicrophoneEnabled && (
                  <button
                    className="lk-button px-2 text-xs"
                    aria-label={`Mute ${name}`}
                    disabled={mute.isPending}
                    onClick={() => mute.mutate(p.identity)}
                  >
                    Mute
                  </button>
                )}
                <button
                  className="lk-button px-2 text-xs"
                  aria-label={`Remove ${name}`}
                  disabled={remove.isPending}
                  onClick={() => {
                    if (confirm(`Remove ${name} from the meeting?`)) remove.mutate(p.identity);
                  }}
                >
                  Remove
                </button>
              </>
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
