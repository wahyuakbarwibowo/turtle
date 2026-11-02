"use client";

import "@livekit/components-styles";

import {
  LiveKitRoom,
  useLocalParticipant,
  useRoomContext,
} from "@livekit/components-react";
import { DisconnectReason, RoomEvent, VideoPresets } from "livekit-client";
import { useParams } from "next/navigation";
import { useEffect, useState } from "react";

import { CallLayout } from "@/components/call/call-layout";
import { type JoinInfo, api } from "@/lib/api";

/**
 * Meeting UI for iframes on third-party sites (see public/sdk/turtlemeet-embed.js).
 * The embed token arrives in the URL fragment (`#t=…`) so it never hits server logs.
 *
 * postMessage protocol, all messages `{ source: "turtlemeet", type, ... }`:
 *   parent → iframe: hello (handshake, fixes the parent origin), command { name }
 *   iframe → parent: ready, joined, left, participantJoined, participantLeft,
 *                    audioMuteStatusChanged, videoMuteStatusChanged, readyToClose, error
 */

type Outgoing = { type: string } & Record<string, unknown>;

// The parent's origin is learned from its first `hello` (event.source === window.parent
// can't be spoofed by other frames). Events before the handshake are buffered.
let parentOrigin: string | null = null;
const pending: Outgoing[] = [];
function emit(msg: Outgoing) {
  if (window.parent === window) return;
  if (!parentOrigin) return void pending.push(msg);
  window.parent.postMessage({ source: "turtlemeet", ...msg }, parentOrigin);
}

function useParentHandshake() {
  useEffect(() => {
    function onMessage(e: MessageEvent) {
      if (
        e.source !== window.parent ||
        e.data?.source !== "turtlemeet" ||
        e.data.type !== "hello"
      )
        return;
      if (parentOrigin) return;
      parentOrigin = e.origin;
      pending.splice(0).forEach(emit);
    }
    window.addEventListener("message", onMessage);
    // Carries no data, so "*" is safe; it prompts the SDK to answer with hello.
    if (window.parent !== window) window.parent.postMessage({ source: "turtlemeet", type: "ready" }, "*");
    return () => window.removeEventListener("message", onMessage);
  }, []);
}

type Phase =
  | { kind: "joining" }
  | { kind: "in-call"; join: JoinInfo }
  | { kind: "left"; reason?: DisconnectReason }
  | { kind: "error"; message: string };

export function EmbedRoom() {
  const { code } = useParams<{ code: string }>();
  const [phase, setPhase] = useState<Phase>({ kind: "joining" });
  useParentHandshake();

  useEffect(() => {
    const token = new URLSearchParams(window.location.hash.slice(1)).get("t");
    // Drop the token from the address bar/history once read.
    history.replaceState(
      null,
      "",
      window.location.pathname + window.location.search,
    );
    (token
      ? api.join(code, { embed_token: token })
      : Promise.reject(new Error("Missing embed token"))
    ).then(
      (o) =>
        o.status === "admitted"
          ? setPhase({ kind: "in-call", join: o })
          : setPhase({ kind: "error", message: "Unexpected lobby" }),
      (e: Error) => setPhase({ kind: "error", message: e.message }),
    );
  }, [code]);

  useEffect(() => {
    if (phase.kind === "error") emit({ type: "error", message: phase.message });
    if (phase.kind === "left")
      emit({ type: "readyToClose", reason: phase.reason });
  }, [phase]);

  if (phase.kind === "joining") return <Centered>Joining…</Centered>;
  if (phase.kind === "error") return <Centered>{phase.message}</Centered>;
  if (phase.kind === "left") return <Centered>You left the meeting</Centered>;

  const q = new URLSearchParams(window.location.search);
  return (
    <LiveKitRoom
      data-lk-theme="default"
      style={{ height: "100dvh", position: "relative" }}
      serverUrl={phase.join.livekit_url}
      token={phase.join.token}
      connect
      audio={q.get("audio") !== "0"}
      video={q.get("video") !== "0"}
      options={{
        adaptiveStream: true,
        dynacast: true,
        publishDefaults: {
          simulcast: true,
          videoSimulcastLayers: [VideoPresets.h180, VideoPresets.h360],
        },
      }}
      onDisconnected={(reason) => {
        emit({ type: "left", reason });
        setPhase({ kind: "left", reason });
      }}
    >
      <EmbedBridge join={phase.join} />
      <CallLayout join={phase.join} embedded />
    </LiveKitRoom>
  );
}

/** Relays room events to the parent and runs its commands. */
function EmbedBridge({ join }: { join: JoinInfo }) {
  const room = useRoomContext();
  const { localParticipant, isMicrophoneEnabled, isCameraEnabled } =
    useLocalParticipant();

  useEffect(() => {
    const who = (p: { identity: string; name?: string }) => ({
      identity: p.identity,
      name: p.name,
    });
    const onConnected = () =>
      emit({
        type: "joined",
        identity: join.identity,
        role: join.role,
        meeting: join.meeting.code,
      });
    const onJoin = (p: { identity: string; name?: string }) =>
      emit({ type: "participantJoined", ...who(p) });
    const onLeave = (p: { identity: string; name?: string }) =>
      emit({ type: "participantLeft", ...who(p) });
    if (room.state === "connected") onConnected();
    room.on(RoomEvent.Connected, onConnected);
    room.on(RoomEvent.ParticipantConnected, onJoin);
    room.on(RoomEvent.ParticipantDisconnected, onLeave);
    return () => {
      room.off(RoomEvent.Connected, onConnected);
      room.off(RoomEvent.ParticipantConnected, onJoin);
      room.off(RoomEvent.ParticipantDisconnected, onLeave);
    };
  }, [room, join]);

  useEffect(
    () => emit({ type: "audioMuteStatusChanged", muted: !isMicrophoneEnabled }),
    [isMicrophoneEnabled],
  );
  useEffect(
    () => emit({ type: "videoMuteStatusChanged", muted: !isCameraEnabled }),
    [isCameraEnabled],
  );

  useEffect(() => {
    function onMessage(e: MessageEvent) {
      if (
        e.source !== window.parent ||
        e.origin !== parentOrigin ||
        e.data?.source !== "turtlemeet"
      )
        return;
      if (e.data.type !== "command") return;
      const lp = localParticipant;
      const run: Record<string, () => Promise<unknown> | void> = {
        hangup: () => room.disconnect(),
        toggleAudio: () => lp.setMicrophoneEnabled(!lp.isMicrophoneEnabled),
        toggleVideo: () => lp.setCameraEnabled(!lp.isCameraEnabled),
        toggleShareScreen: () =>
          lp.setScreenShareEnabled(!lp.isScreenShareEnabled),
      };
      const fn = run[e.data.name as string];
      if (!fn)
        return emit({
          type: "error",
          message: `unknown command ${e.data.name}`,
        });
      Promise.resolve(fn()).catch((err: Error) =>
        emit({ type: "error", message: err.message }),
      );
    }
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [room, localParticipant]);

  return null;
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex min-h-dvh flex-1 items-center justify-center p-6 text-center text-lg">
      {children}
    </div>
  );
}
