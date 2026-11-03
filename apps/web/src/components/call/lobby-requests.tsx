"use client";

import { useDataChannel } from "@livekit/components-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { api } from "@/lib/api";

/** Must match LOBBY_TOPIC in apps/server/src/meetings/service.rs. */
const LOBBY_TOPIC = "turtlemeet.lobby";

/**
 * Host-only card listing people waiting in the lobby. A data-channel ping triggers an
 * immediate refresh; the interval is the fallback (and drops abandoned requests).
 */
export function LobbyRequests({ code, participantToken }: { code: string; participantToken: string }) {
  const qc = useQueryClient();
  const key = ["lobby", code];
  const lobby = useQuery({
    queryKey: key,
    queryFn: () => api.lobby(code, participantToken),
    refetchInterval: 5000,
  });
  // Server-sent only; ignore participants publishing on the topic.
  useDataChannel(LOBBY_TOPIC, (msg) => {
    if (!msg.from) qc.invalidateQueries({ queryKey: key });
  });

  const decide = useMutation({
    mutationFn: ({ identity, admit }: { identity: string; admit: boolean }) =>
      admit ? api.admit(code, participantToken, identity) : api.deny(code, participantToken, identity),
    onSettled: () => qc.invalidateQueries({ queryKey: key }),
    onError: (e) => toast.error(e.message),
  });

  const waiting = lobby.data ?? [];
  if (waiting.length === 0) return null;

  return (
    <div
      role="region"
      aria-label="Lobby requests"
      className="absolute top-3 left-1/2 z-20 w-80 -translate-x-1/2 rounded-lg border border-white/10 p-3 shadow-lg"
      style={{ background: "var(--lk-bg2)" }}
    >
      <p className="mb-2 text-sm font-medium">
        {waiting.length === 1 ? "1 person wants to join" : `${waiting.length} people want to join`}
      </p>
      <ul className="space-y-2">
        {waiting.map((w) => (
          <li key={w.identity} className="flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-sm">{w.display_name}</span>
            <button
              className="lk-button text-xs"
              disabled={decide.isPending}
              onClick={() => decide.mutate({ identity: w.identity, admit: false })}
            >
              Deny
            </button>
            <button
              className="lk-button text-xs"
              style={{ background: "var(--lk-accent-bg)" }}
              disabled={decide.isPending}
              onClick={() => decide.mutate({ identity: w.identity, admit: true })}
            >
              Admit
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
