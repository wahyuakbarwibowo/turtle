import { z } from "zod";

export const userSchema = z.object({
  id: z.uuid(),
  email: z.string(),
  display_name: z.string(),
  role: z.enum(["user", "admin"]),
  created_at: z.string(),
});
export type User = z.infer<typeof userSchema>;

export const meetingSchema = z.object({
  id: z.uuid(),
  code: z.string(),
  title: z.string(),
  host_id: z.uuid(),
  started_at: z.string().nullable(),
  ended_at: z.string().nullable(),
  created_at: z.string(),
});
export type Meeting = z.infer<typeof meetingSchema>;

export const joinInfoSchema = z.object({
  token: z.string(),
  livekit_url: z.string(),
  identity: z.string(),
  role: z.enum(["host", "participant"]),
  meeting: meetingSchema,
});
export type JoinInfo = z.infer<typeof joinInfoSchema>;

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

const errorSchema = z.object({ error: z.object({ code: z.string(), message: z.string() }) });

async function request<T>(path: string, schema: z.ZodType<T> | null, init?: { method?: string; body?: unknown }): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method: init?.method ?? "GET",
    headers: init?.body !== undefined ? { "content-type": "application/json" } : undefined,
    body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
    credentials: "same-origin",
  });
  if (!res.ok) {
    const parsed = errorSchema.safeParse(await res.json().catch(() => null));
    throw parsed.success
      ? new ApiError(res.status, parsed.data.error.code, parsed.data.error.message)
      : new ApiError(res.status, "unknown", `Request failed (${res.status})`);
  }
  return schema ? schema.parse(await res.json()) : (undefined as T);
}

export const api = {
  me: () => request("/auth/me", userSchema),
  register: (body: { email: string; password: string; display_name: string }) =>
    request("/auth/register", userSchema, { method: "POST", body }),
  login: (body: { email: string; password: string }) => request("/auth/login", userSchema, { method: "POST", body }),
  logout: () => request("/auth/logout", null, { method: "POST" }),
  meetings: () => request("/meetings", z.array(meetingSchema)),
  meeting: (code: string) => request(`/meetings/${encodeURIComponent(code)}`, meetingSchema),
  createMeeting: (title: string) => request("/meetings", meetingSchema, { method: "POST", body: { title } }),
  join: (code: string, display_name?: string) =>
    request(`/meetings/${encodeURIComponent(code)}/join`, joinInfoSchema, { method: "POST", body: { display_name } }),
  endMeeting: (code: string) => request(`/meetings/${encodeURIComponent(code)}/end`, null, { method: "POST" }),
  removeParticipant: (code: string, identity: string) =>
    request(
      `/meetings/${encodeURIComponent(code)}/participants/${encodeURIComponent(identity)}/remove`,
      null,
      { method: "POST" },
    ),
};

/** Extracts a meeting code from a raw code or a full invite link. */
export function parseMeetingCode(input: string): string | null {
  const match = input.trim().toLowerCase().match(/([a-z]{3}-[a-z]{4}-[a-z]{3})\/?$/);
  return match ? match[1] : null;
}
