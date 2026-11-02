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
  host_id: z.uuid().nullable(),
  started_at: z.string().nullable(),
  ended_at: z.string().nullable(),
  created_at: z.string(),
  lobby_enabled: z.boolean(),
  has_password: z.boolean(),
});
export type Meeting = z.infer<typeof meetingSchema>;

export const joinInfoSchema = z.object({
  token: z.string(),
  participant_token: z.string(),
  livekit_url: z.string(),
  identity: z.string(),
  role: z.enum(["host", "participant"]),
  meeting: meetingSchema,
});
export type JoinInfo = z.infer<typeof joinInfoSchema>;

export const joinOutcomeSchema = z.discriminatedUnion("status", [
  joinInfoSchema.extend({ status: z.literal("admitted") }),
  z.object({ status: z.literal("waiting"), identity: z.string(), participant_token: z.string() }),
]);
export type JoinOutcome = z.infer<typeof joinOutcomeSchema>;

const createdMeetingSchema = meetingSchema.extend({ host_secret: z.string().optional() });

export const lobbyEntrySchema = z.object({
  identity: z.string(),
  display_name: z.string(),
  requested_at: z.string(),
});
export type LobbyEntry = z.infer<typeof lobbyEntrySchema>;

export const chatMessageSchema = z.object({
  id: z.uuid(),
  identity: z.string().nullable(),
  display_name: z.string().nullable(),
  body: z.string(),
  created_at: z.string(),
});
export type ChatMessage = z.infer<typeof chatMessageSchema>;

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

async function request<T>(
  path: string,
  schema: z.ZodType<T> | null,
  init?: { method?: string; body?: unknown; bearer?: string },
): Promise<T> {
  const headers: Record<string, string> = {};
  if (init?.body !== undefined) headers["content-type"] = "application/json";
  if (init?.bearer) headers.authorization = `Bearer ${init.bearer}`;
  const res = await fetch(`/api${path}`, {
    method: init?.method ?? "GET",
    headers,
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

const m = (code: string) => `/meetings/${encodeURIComponent(code)}`;
const id = encodeURIComponent;

export const api = {
  config: () => request("/config", z.object({ allow_guest_create: z.boolean() })),
  me: () => request("/auth/me", userSchema),
  register: (body: { email: string; password: string; display_name: string }) =>
    request("/auth/register", userSchema, { method: "POST", body }),
  login: (body: { email: string; password: string }) => request("/auth/login", userSchema, { method: "POST", body }),
  logout: () => request("/auth/logout", null, { method: "POST" }),
  meetings: () => request("/meetings", z.array(meetingSchema)),
  meeting: (code: string) => request(m(code), meetingSchema),
  createMeeting: (title: string) => request("/meetings", createdMeetingSchema, { method: "POST", body: { title } }),
  join: (
    code: string,
    body: { display_name?: string; password?: string; host_secret?: string; embed_token?: string },
  ) =>
    request(`${m(code)}/join`, joinOutcomeSchema, { method: "POST", body }),

  // In-meeting actions authenticate with the participant token from join.
  lobbyStatus: (code: string, pt: string) => request(`${m(code)}/lobby/me`, joinOutcomeSchema, { bearer: pt }),
  lobby: (code: string, pt: string) => request(`${m(code)}/lobby`, z.array(lobbyEntrySchema), { bearer: pt }),
  admit: (code: string, pt: string, identity: string) =>
    request(`${m(code)}/lobby/${id(identity)}/admit`, null, { method: "POST", bearer: pt }),
  deny: (code: string, pt: string, identity: string) =>
    request(`${m(code)}/lobby/${id(identity)}/deny`, null, { method: "POST", bearer: pt }),
  updateSettings: (code: string, pt: string, body: { lobby_enabled?: boolean; password?: string | null }) =>
    request(`${m(code)}/settings`, meetingSchema, { method: "PATCH", body, bearer: pt }),
  setHand: (code: string, pt: string, raised: boolean) =>
    request(`${m(code)}/hand`, null, { method: "POST", body: { raised }, bearer: pt }),
  mute: (code: string, pt: string, identity: string) =>
    request(`${m(code)}/participants/${id(identity)}/mute`, null, { method: "POST", bearer: pt }),
  muteAll: (code: string, pt: string) => request(`${m(code)}/mute-all`, null, { method: "POST", bearer: pt }),
  removeParticipant: (code: string, pt: string, identity: string) =>
    request(`${m(code)}/participants/${id(identity)}/remove`, null, { method: "POST", bearer: pt }),
  endMeeting: (code: string, pt: string) => request(`${m(code)}/end`, null, { method: "POST", bearer: pt }),
  chatHistory: (code: string, pt: string) => request(`${m(code)}/chat`, z.array(chatMessageSchema), { bearer: pt }),
  sendChat: (code: string, pt: string, body: string) =>
    request(`${m(code)}/chat`, chatMessageSchema, { method: "POST", body: { body }, bearer: pt }),
};

/** Host secret for meetings created without an account; kept only in this browser. */
export const hostSecret = {
  get: (code: string) => {
    try {
      return localStorage.getItem(`tm_host:${code}`) ?? undefined;
    } catch {
      return undefined;
    }
  },
  set: (code: string, secret: string) => {
    try {
      localStorage.setItem(`tm_host:${code}`, secret);
    } catch {
      // Private mode: the creator just joins as a participant.
    }
  },
};

/** Extracts a meeting code from a raw code or a full invite link. */
export function parseMeetingCode(input: string): string | null {
  const match = input.trim().toLowerCase().match(/([a-z]{3}-[a-z]{4}-[a-z]{3})\/?$/);
  return match ? match[1] : null;
}
