"use client";

import {
  CarouselLayout,
  ConnectionStateToast,
  ControlBar,
  DisconnectButton,
  FocusLayout,
  FocusLayoutContainer,
  GridLayout,
  LayoutContextProvider,
  ParticipantTile,
  RoomAudioRenderer,
  isTrackReference,
  useLocalParticipant,
  useParticipants,
  useTracks,
} from "@livekit/components-react";
import { useMutation } from "@tanstack/react-query";
import { Hand, Link2, MessageSquare, PhoneOff, Users } from "lucide-react";
import { RoomEvent, Track } from "livekit-client";
import { useState } from "react";
import { toast } from "sonner";

import { type JoinInfo, api } from "@/lib/api";

import { LobbyRequests } from "./lobby-requests";
import { ReactionOverlay, ReactionPicker, useReactions } from "./reactions";
import { type Panel, SidePanel } from "./side-panel";
import { useChat } from "./use-chat";

/** Grid of cameras; switches to a focus layout while anyone shares their screen. */
function Stage() {
  const tracks = useTracks(
    [
      { source: Track.Source.Camera, withPlaceholder: true },
      { source: Track.Source.ScreenShare, withPlaceholder: false },
    ],
    { updateOnlyOn: [RoomEvent.ActiveSpeakersChanged], onlySubscribed: false },
  );
  const screen = tracks.find((t) => isTrackReference(t) && t.source === Track.Source.ScreenShare);

  if (screen) {
    return (
      <FocusLayoutContainer className="h-full">
        <CarouselLayout tracks={tracks.filter((t) => t !== screen)}>
          <ParticipantTile />
        </CarouselLayout>
        <FocusLayout trackRef={screen} />
      </FocusLayoutContainer>
    );
  }
  return (
    <GridLayout tracks={tracks} className="h-full">
      <ParticipantTile />
    </GridLayout>
  );
}

export function CallLayout({ join }: { join: JoinInfo }) {
  const code = join.meeting.code;
  const isHost = join.role === "host";
  const [panel, setPanel] = useState<Panel | null>(null);
  const chat = useChat(code, join.participant_token, panel === "chat");
  const participants = useParticipants();
  const raisedHands = participants.filter((p) => p.attributes.hand === "1").length;
  const { localParticipant } = useLocalParticipant();
  const handUp = localParticipant.attributes.hand === "1";
  const { floating, react } = useReactions();

  const end = useMutation({
    mutationFn: () => api.endMeeting(code, join.participant_token),
    onError: (e) => toast.error(e.message),
  });
  const hand = useMutation({
    mutationFn: (raised: boolean) => api.setHand(code, join.participant_token, raised),
    onError: (e) => toast.error(e.message),
  });

  function toggle(p: Panel) {
    if (p === "chat") chat.markRead();
    setPanel((cur) => (cur === p ? null : p));
  }

  async function copyInvite() {
    await navigator.clipboard.writeText(`${window.location.origin}/m/${code}`);
    toast.success("Invite link copied");
  }

  return (
    <LayoutContextProvider>
      <div className="flex h-full min-h-0">
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="relative min-h-0 flex-1 p-2">
            <Stage />
            <ReactionOverlay floating={floating} />
            {isHost && <LobbyRequests code={code} participantToken={join.participant_token} />}
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2 p-2">
            <ControlBar
              controls={{ chat: false, settings: false, leave: false }}
              style={{ border: "none", padding: 0 }}
            />
            <button
              className="lk-button"
              aria-pressed={handUp}
              disabled={hand.isPending}
              onClick={() => hand.mutate(!handUp)}
              style={handUp ? { background: "var(--lk-accent-bg)" } : undefined}
            >
              <Hand className="size-4" /> {handUp ? "Lower hand" : "Raise hand"}
            </button>
            <ReactionPicker onReact={react} />
            <button
              className="lk-button"
              aria-label={`People (${participants.length})`}
              aria-pressed={panel === "people"}
              onClick={() => toggle("people")}
            >
              <Users className="size-4" /> {participants.length}
              {raisedHands > 0 && (
                <span className="flex items-center gap-0.5 rounded-full bg-amber-500 px-1.5 text-xs text-black">
                  <Hand className="size-3" aria-hidden /> {raisedHands}
                </span>
              )}
            </button>
            <button className="lk-button" aria-pressed={panel === "chat"} onClick={() => toggle("chat")}>
              <MessageSquare className="size-4" />
              Chat
              {chat.unread > 0 && panel !== "chat" && (
                <span className="rounded-full bg-emerald-600 px-1.5 text-xs" aria-label={`${chat.unread} unread`}>
                  {chat.unread}
                </span>
              )}
            </button>
            <button className="lk-button" onClick={copyInvite} aria-label="Copy invite link">
              <Link2 className="size-4" />
            </button>
            <DisconnectButton>Leave</DisconnectButton>
            {isHost && (
              <button
                className="lk-button lk-disconnect-button"
                disabled={end.isPending}
                onClick={() => {
                  if (confirm("End the meeting for everyone?")) end.mutate();
                }}
              >
                <PhoneOff className="size-4" /> End for all
              </button>
            )}
          </div>
        </div>
        {panel && (
          <SidePanel
            panel={panel}
            onClose={() => setPanel(null)}
            code={code}
            participantToken={join.participant_token}
            isHost={isHost}
            meeting={join.meeting}
            chat={chat}
          />
        )}
      </div>
      <RoomAudioRenderer />
      <ConnectionStateToast />
    </LayoutContextProvider>
  );
}
