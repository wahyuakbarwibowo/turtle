-- Lobby, meeting passwords, and meetings created without an account.

-- Guest-created meetings have no host user; the creator proves host rights with a secret
-- (only its SHA-256 is stored).
ALTER TABLE meetings ALTER COLUMN host_id DROP NOT NULL;
ALTER TABLE meetings ADD COLUMN host_secret_hash BYTEA;
ALTER TABLE meetings ADD CONSTRAINT meetings_has_host
    CHECK (host_id IS NOT NULL OR host_secret_hash IS NOT NULL);

-- Argon2id hash; NULL means no password.
ALTER TABLE meetings ADD COLUMN password_hash TEXT;
ALTER TABLE meetings ADD COLUMN lobby_enabled BOOLEAN NOT NULL DEFAULT false;

-- 'waiting' rows are lobby requests; only 'admitted' participants may act in the meeting.
CREATE TYPE participant_status AS ENUM ('waiting', 'admitted', 'denied');
ALTER TABLE meeting_participants
    ADD COLUMN status participant_status NOT NULL DEFAULT 'admitted';

CREATE INDEX meeting_participants_identity_idx
    ON meeting_participants(meeting_id, identity, joined_at DESC);
CREATE INDEX meeting_participants_waiting_idx
    ON meeting_participants(meeting_id) WHERE status = 'waiting';
