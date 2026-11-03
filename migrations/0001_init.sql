-- TurtleMeet initial schema. gen_random_uuid() is built into PostgreSQL 13+.

CREATE EXTENSION IF NOT EXISTS citext;

CREATE TYPE user_role AS ENUM ('user', 'admin');
CREATE TYPE participant_role AS ENUM ('host', 'participant');

CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         CITEXT NOT NULL UNIQUE CHECK (length(email) BETWEEN 3 AND 254),
    display_name  TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 80),
    password_hash TEXT NOT NULL,
    role          user_role NOT NULL DEFAULT 'user',
    disabled_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Only a SHA-256 of the session token is stored; the raw token lives in the cookie.
CREATE TABLE sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  BYTEA NOT NULL UNIQUE,
    user_agent  TEXT,
    ip          INET,
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_id_idx ON sessions(user_id);

CREATE TABLE meetings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Public, URL-safe room code used in invite links (e.g. abc-defg-hij).
    code        TEXT NOT NULL UNIQUE CHECK (code ~ '^[a-z0-9-]{6,32}$'),
    title       TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 120),
    host_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    started_at  TIMESTAMPTZ,
    ended_at    TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (ended_at IS NULL OR started_at IS NULL OR ended_at >= started_at)
);
CREATE INDEX meetings_host_id_idx ON meetings(host_id, created_at DESC);

CREATE TABLE meeting_participants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meeting_id  UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    user_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    -- LiveKit participant identity; guests have no user_id.
    identity    TEXT NOT NULL,
    display_name TEXT NOT NULL CHECK (length(display_name) BETWEEN 1 AND 80),
    role        participant_role NOT NULL DEFAULT 'participant',
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    left_at     TIMESTAMPTZ,
    removed_at  TIMESTAMPTZ
);
CREATE INDEX meeting_participants_meeting_idx ON meeting_participants(meeting_id);
CREATE INDEX meeting_participants_user_idx ON meeting_participants(user_id, joined_at DESC);

CREATE TABLE meeting_invitations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meeting_id  UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    email       CITEXT NOT NULL,
    invited_by  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (meeting_id, email)
);

CREATE TABLE chat_messages (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meeting_id  UUID NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    participant_id UUID REFERENCES meeting_participants(id) ON DELETE SET NULL,
    body        TEXT NOT NULL CHECK (length(body) BETWEEN 1 AND 4000),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_messages_meeting_idx ON chat_messages(meeting_id, created_at);

CREATE TABLE audit_logs (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor_id    UUID REFERENCES users(id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target_type TEXT,
    target_id   UUID,
    metadata    JSONB NOT NULL DEFAULT '{}',
    ip          INET,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_logs_actor_idx ON audit_logs(actor_id, created_at DESC);
CREATE INDEX audit_logs_created_idx ON audit_logs(created_at DESC);
