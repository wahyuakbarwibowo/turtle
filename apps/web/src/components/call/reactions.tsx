"use client";

import { useDataChannel, useParticipants } from "@livekit/components-react";
import { SmilePlus } from "lucide-react";
import { useRef, useState } from "react";

const REACTION_TOPIC = "turtlemeet.reaction";
const EMOJIS = ["👍", "👏", "😂", "🎉", "❤️"] as const;
const SHOW_MS = 3000;

type Floating = { id: number; emoji: string; from: string; left: number };
const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * Ephemeral emoji reactions over LiveKit data (lossy: a dropped reaction is harmless).
 * Only whitelisted emoji are rendered, so a client can't inject arbitrary text.
 */
export function useReactions() {
  const [floating, setFloating] = useState<Floating[]>([]);
  const nextId = useRef(0);
  const participants = useParticipants();

  function show(emoji: string, from: string) {
    const id = nextId.current++;
    setFloating((f) => [...f, { id, emoji, from, left: 10 + Math.random() * 30 }]);
    setTimeout(() => setFloating((f) => f.filter((x) => x.id !== id)), SHOW_MS);
  }

  const { send } = useDataChannel(REACTION_TOPIC, (msg) => {
    const emoji = decoder.decode(msg.payload);
    if (!(EMOJIS as readonly string[]).includes(emoji)) return;
    const sender = participants.find((p) => p.identity === msg.from?.identity);
    show(emoji, sender?.name || "Someone");
  });

  function react(emoji: string) {
    send(encoder.encode(emoji), { reliable: false });
    show(emoji, "You");
  }

  return { floating, react };
}

export function ReactionPicker({ onReact }: { onReact: (emoji: string) => void }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="relative">
      <button className="lk-button" aria-expanded={open} aria-label="Reactions" onClick={() => setOpen((o) => !o)}>
        <SmilePlus className="size-4" />
      </button>
      {open && (
        <div
          className="absolute bottom-full left-1/2 mb-2 flex -translate-x-1/2 gap-1 rounded-lg border border-white/10 p-1"
          style={{ background: "var(--lk-bg2)" }}
        >
          {EMOJIS.map((e) => (
            <button
              key={e}
              className="lk-button px-2 text-lg"
              aria-label={`React ${e}`}
              onClick={() => {
                onReact(e);
                setOpen(false);
              }}
            >
              {e}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function ReactionOverlay({ floating }: { floating: Floating[] }) {
  return (
    <div aria-live="polite" className="pointer-events-none absolute inset-0 z-10 overflow-hidden">
      {floating.map((f) => (
        <div
          key={f.id}
          className="absolute bottom-20 flex flex-col items-center motion-safe:animate-[tm-float_3s_ease-out_forwards]"
          style={{ left: `${f.left}%` }}
        >
          <span className="text-4xl">{f.emoji}</span>
          <span className="rounded bg-black/60 px-1.5 text-xs">{f.from}</span>
        </div>
      ))}
    </div>
  );
}
