"use client";

import "@livekit/components-styles";

import { LiveKitRoom, type LocalUserChoices, PreJoin } from "@livekit/components-react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { DisconnectReason, type RoomOptions, VideoPresets } from "livekit-client";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { CallLayout } from "@/components/call/call-layout";
import { Logo } from "@/components/site-header";
import { Button } from "@/components/ui/button";
import { ApiError, type JoinInfo, api } from "@/lib/api";
import { useMe } from "@/lib/session";

// Simulcast publishes 3 layers; adaptiveStream subscribes to the layer that fits each tile,
// and dynacast pauses layers nobody watches. Together they keep large rooms affordable.
const ROOM_OPTIONS: RoomOptions = {
  adaptiveStream: true,
  dynacast: true,
  videoCaptureDefaults: { resolution: VideoPresets.h720.resolution },
  publishDefaults: {
    simulcast: true,
    videoSimulcastLayers: [VideoPresets.h180, VideoPresets.h360],
  },
};

type Phase =
  | { kind: "prejoin" }
  | { kind: "in-call"; join: JoinInfo; choices: LocalUserChoices }
  | { kind: "left"; reason?: DisconnectReason };

export function MeetingRoom() {
  const { code } = useParams<{ code: string }>();
  const { data: me, isPending: meLoading } = useMe();
  const [phase, setPhase] = useState<Phase>({ kind: "prejoin" });

  const meeting = useQuery({ queryKey: ["meeting", code], queryFn: () => api.meeting(code), retry: false });

  const join = useMutation({
    mutationFn: (choices: LocalUserChoices) => api.join(code, choices.username),
    onSuccess: (join, choices) => setPhase({ kind: "in-call", join, choices }),
    onError: (e) => toast.error(e.message),
  });

  if (meeting.isPending || meLoading) return <Centered>Loading…</Centered>;

  if (meeting.isError) {
    const notFound = meeting.error instanceof ApiError && meeting.error.status === 404;
    return (
      <Centered>
        <p className="text-lg font-medium">{notFound ? "Meeting not found" : "Couldn't load this meeting"}</p>
        <p className="text-muted-foreground">Check the link and try again.</p>
        <HomeButton />
      </Centered>
    );
  }

  if (meeting.data.ended_at) {
    return (
      <Centered>
        <p className="text-lg font-medium">This meeting has ended</p>
        <HomeButton />
      </Centered>
    );
  }

  if (phase.kind === "left") {
    const message =
      phase.reason === DisconnectReason.PARTICIPANT_REMOVED
        ? "You were removed from the meeting by the host"
        : phase.reason === DisconnectReason.ROOM_DELETED
          ? "The host ended the meeting"
          : "You left the meeting";
    const canRejoin = phase.reason !== DisconnectReason.PARTICIPANT_REMOVED && phase.reason !== DisconnectReason.ROOM_DELETED;
    return (
      <Centered>
        <p className="text-lg font-medium">{message}</p>
        <div className="flex gap-2">
          {canRejoin && <Button onClick={() => setPhase({ kind: "prejoin" })}>Rejoin</Button>}
          <HomeButton />
        </div>
      </Centered>
    );
  }

  if (phase.kind === "prejoin") {
    return (
      <div data-lk-theme="default" className="flex flex-col" style={{ minHeight: "100dvh", background: "var(--lk-bg)" }}>
        <div className="mx-auto flex w-full max-w-6xl items-center justify-between px-4 py-4">
          <Logo />
        </div>
        <div className="flex flex-1 flex-col items-center justify-center gap-4 px-4 pb-16">
          <div className="text-center">
            <h1 className="text-2xl font-semibold">{meeting.data.title}</h1>
            <p className="font-mono text-sm opacity-70">{meeting.data.code}</p>
          </div>
          <PreJoin
            // Signed-in users join under their account name; guests must type one.
            defaults={{ username: me?.display_name ?? "" }}
            joinLabel={join.isPending ? "Joining…" : "Join now"}
            userLabel="Your name"
            onValidate={(v) => v.username.trim().length > 0 && !join.isPending}
            onSubmit={(choices) => join.mutate(choices)}
            onError={(e) => toast.error(`Camera/microphone: ${e.message}`)}
          />
        </div>
      </div>
    );
  }

  return <InCall join={phase.join} choices={phase.choices} onLeave={(reason) => setPhase({ kind: "left", reason })} />;
}

function InCall({
  join,
  choices,
  onLeave,
}: {
  join: JoinInfo;
  choices: LocalUserChoices;
  onLeave: (reason?: DisconnectReason) => void;
}) {
  const audio = useMemo(
    () => (choices.audioEnabled ? { deviceId: choices.audioDeviceId || undefined } : false),
    [choices.audioEnabled, choices.audioDeviceId],
  );
  const video = useMemo(
    () => (choices.videoEnabled ? { deviceId: choices.videoDeviceId || undefined } : false),
    [choices.videoEnabled, choices.videoDeviceId],
  );

  return (
    <LiveKitRoom
      data-lk-theme="default"
      // Inline: LiveKit's unlayered CSS overrides Tailwind's layered utilities.
      style={{ height: "100dvh", position: "relative" }}
      serverUrl={join.livekit_url}
      token={join.token}
      connect
      audio={audio}
      video={video}
      options={ROOM_OPTIONS}
      onDisconnected={onLeave}
      onError={(e) => toast.error(e.message)}
    >
      <CallLayout join={join} />
    </LiveKitRoom>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return <main className="flex min-h-dvh flex-col items-center justify-center gap-3 px-4 text-center">{children}</main>;
}

function HomeButton() {
  return (
    <Button variant="ghost" render={<Link href="/" />}>
      Back to home
    </Button>
  );
}
