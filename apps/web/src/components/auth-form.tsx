"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { useState } from "react";

import { Logo } from "@/components/site-header";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";

/** Only same-origin relative paths, to avoid open redirects via ?next=. */
function safeNext(next: string | null): string {
  return next && next.startsWith("/") && !next.startsWith("//") ? next : "/";
}

export function AuthForm({ mode }: { mode: "login" | "register" }) {
  const router = useRouter();
  const params = useSearchParams();
  const qc = useQueryClient();
  const next = safeNext(params.get("next"));
  const [form, setForm] = useState({ email: "", password: "", display_name: "" });

  const submit = useMutation({
    mutationFn: () =>
      mode === "login" ? api.login({ email: form.email, password: form.password }) : api.register(form),
    onSuccess: (user) => {
      qc.setQueryData(["me"], user);
      router.replace(next);
    },
  });

  const field = (key: keyof typeof form) => ({
    id: key,
    value: form[key],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [key]: e.target.value }),
  });
  const isLogin = mode === "login";
  const other = `${isLogin ? "/register" : "/login"}${next !== "/" ? `?next=${encodeURIComponent(next)}` : ""}`;

  return (
    <main className="flex min-h-dvh flex-col items-center justify-center gap-6 px-4">
      <Logo />
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>{isLogin ? "Sign in" : "Create your account"}</CardTitle>
          <CardDescription>{isLogin ? "Welcome back." : "Host meetings and keep your history."}</CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-4"
            onSubmit={(e) => {
              e.preventDefault();
              submit.mutate();
            }}
          >
            {!isLogin && (
              <div className="grid gap-2">
                <Label htmlFor="display_name">Name</Label>
                <Input {...field("display_name")} required maxLength={80} autoComplete="name" />
              </div>
            )}
            <div className="grid gap-2">
              <Label htmlFor="email">Email</Label>
              <Input {...field("email")} type="email" required autoComplete="email" />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input
                {...field("password")}
                type="password"
                required
                minLength={isLogin ? undefined : 8}
                maxLength={128}
                autoComplete={isLogin ? "current-password" : "new-password"}
              />
            </div>
            {submit.error && (
              <p role="alert" className="text-sm text-destructive">
                {submit.error.message}
              </p>
            )}
            <Button type="submit" className="h-10" disabled={submit.isPending}>
              {submit.isPending ? "Please wait…" : isLogin ? "Sign in" : "Create account"}
            </Button>
          </form>
          <p className="mt-4 text-center text-sm text-muted-foreground">
            {isLogin ? "New here? " : "Already have an account? "}
            <Link href={other} className="font-medium text-foreground underline-offset-4 hover:underline">
              {isLogin ? "Create an account" : "Sign in"}
            </Link>
          </p>
        </CardContent>
      </Card>
    </main>
  );
}
