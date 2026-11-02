"use client";

import { useDataChannel } from "@livekit/components-react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { type ChatMessage, api, chatMessageSchema } from "@/lib/api";

/** Must match CHAT_TOPIC in apps/server/src/chat/mod.rs. */
const CHAT_TOPIC = "turtlemeet.chat";
const decoder = new TextDecoder();

/**
 * History comes from the API; new messages arrive live over LiveKit's data channel
 * (the server broadcasts after persisting, including back to the sender).
 */
export function useChat(code: string, participantToken: string, panelOpen: boolean) {
  const qc = useQueryClient();
  const key = ["chat", code];
  const [unread, setUnread] = useState(0);

  const history = useQuery({ queryKey: key, queryFn: () => api.chatHistory(code, participantToken) });

  const append = useCallback(
    (msg: ChatMessage) =>
      qc.setQueryData<ChatMessage[]>(["chat", code], (old = []) =>
        old.some((m) => m.id === msg.id) ? old : [...old, msg],
      ),
    [qc, code],
  );

  useDataChannel(CHAT_TOPIC, (packet) => {
    const parsed = chatMessageSchema.safeParse(JSON.parse(decoder.decode(packet.payload)));
    if (!parsed.success) return;
    append(parsed.data);
    if (!panelOpen) setUnread((n) => n + 1);
  });

  const send = useMutation({
    mutationFn: (body: string) => api.sendChat(code, participantToken, body),
    onSuccess: append,
  });

  return {
    messages: history.data ?? [],
    loading: history.isPending,
    send,
    unread,
    markRead: () => setUnread(0),
  };
}
