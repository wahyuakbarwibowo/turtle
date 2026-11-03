import { Suspense } from "react";

import { MeetingRoom } from "@/components/meeting-room";

export default function Page() {
  return (
    <Suspense>
      <MeetingRoom />
    </Suspense>
  );
}
