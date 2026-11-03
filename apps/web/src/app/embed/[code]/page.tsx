import { Suspense } from "react";

import { EmbedRoom } from "@/components/embed-room";

export default function Page() {
  return (
    <Suspense>
      <EmbedRoom />
    </Suspense>
  );
}
