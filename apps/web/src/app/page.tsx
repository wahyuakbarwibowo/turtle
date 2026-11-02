"use client";

import { useMutation } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { toast } from "sonner";

import { SiteHeader } from "@/components/site-header";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api, parseMeetingCode } from "@/lib/api";
import { useMe } from "@/lib/session";

export default function Home() {
  const router = useRouter();
  const { data: me } = useMe();
  const [joinInput, setJoinInput] = useState("");

  const create = useMutation({
    mutationFn: () => api.createMeeting(`${me?.display_name ?? "My"}'s meeting`),
    onSuccess: (m) => router.push(`/m/${m.code}`),
    onError: (e) => toast.error(e.message),
  });

  function startMeeting() {
    if (!me) return router.push("/login?next=/");
    create.mutate();
  }

  function join(e: React.FormEvent) {
    e.preventDefault();
    const code = parseMeetingCode(joinInput);
    if (!code) return toast.error("Enter a meeting code like abc-defg-hij or an invite link");
    router.push(`/m/${code}`);
  }

  return (
    <div className="flex min-h-dvh flex-col">
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-1 flex-col justify-center px-4 pb-24">
        <h1 className="max-w-2xl text-4xl font-semibold tracking-tight text-balance sm:text-5xl">
          Video meetings you host yourself.
        </h1>
        <p className="mt-4 max-w-xl text-lg text-muted-foreground">
          Secure, self-hosted conferencing for teams. No downloads, just share a link.
        </p>
        <div className="mt-10 flex flex-col gap-3 sm:flex-row sm:items-center">
          <Button size="lg" className="h-11 px-5" onClick={startMeeting} disabled={create.isPending}>
            {create.isPending ? "Creating…" : "New meeting"}
          </Button>
          <form onSubmit={join} className="flex gap-2">
            <Input
              aria-label="Meeting code or link"
              placeholder="Enter a code or link"
              value={joinInput}
              onChange={(e) => setJoinInput(e.target.value)}
              className="h-11 w-64"
            />
            <Button type="submit" variant="ghost" size="lg" className="h-11" disabled={!joinInput.trim()}>
              Join
            </Button>
          </form>
        </div>
      </main>
    </div>
  );
}
