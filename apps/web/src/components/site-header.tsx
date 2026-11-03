"use client";

import { useQueryClient } from "@tanstack/react-query";
import Link from "next/link";

import { Button, buttonVariants } from "@/components/ui/button";
import { api } from "@/lib/api";
import { useMe } from "@/lib/session";

export function Logo() {
  return (
    <Link href="/" className="flex items-center gap-2 font-semibold tracking-tight">
      <span aria-hidden className="grid size-7 place-items-center rounded-lg bg-emerald-600 text-sm text-white">
        T
      </span>
      TurtleMeet
    </Link>
  );
}

export function SiteHeader() {
  const { data: me } = useMe();
  const qc = useQueryClient();

  async function signOut() {
    await api.logout();
    qc.setQueryData(["me"], null);
  }

  return (
    <header className="mx-auto flex w-full max-w-6xl items-center justify-between px-4 py-4">
      <Logo />
      <nav className="flex items-center gap-2 text-sm">
        {me ? (
          <>
            <span className="hidden text-muted-foreground sm:inline">{me.display_name}</span>
            <Button variant="ghost" onClick={signOut}>
              Sign out
            </Button>
          </>
        ) : me === null ? (
          <>
            {/* Real links styled as buttons: navigation keeps link semantics for screen readers. */}
            <Link href="/login" className={buttonVariants({ variant: "ghost" })}>
              Sign in
            </Link>
            <Link href="/register" className={buttonVariants()}>
              Sign up
            </Link>
          </>
        ) : null}
      </nav>
    </header>
  );
}
