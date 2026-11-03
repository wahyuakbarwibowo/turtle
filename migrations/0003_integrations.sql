-- Third-party platforms that create meetings and embed TurtleMeet server-to-server.

CREATE TABLE integrations (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name           TEXT NOT NULL UNIQUE CHECK (name ~ '^[a-z0-9][a-z0-9-]{1,31}$'),
    -- SHA-256 of the API key; the key itself is shown once at creation.
    api_key_hash   BYTEA NOT NULL UNIQUE,
    -- Where meeting events are POSTed (optional). Signed with webhook_secret (HMAC-SHA256),
    -- which must be stored in plaintext to sign.
    webhook_url    TEXT CHECK (webhook_url ~ '^https?://'),
    webhook_secret TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at     TIMESTAMPTZ,
    CHECK ((webhook_url IS NULL) = (webhook_secret IS NULL))
);

ALTER TABLE meetings ADD COLUMN integration_id UUID REFERENCES integrations(id) ON DELETE SET NULL;
-- The integrator's own reference (e.g. a ticket id), echoed back in webhooks.
ALTER TABLE meetings ADD COLUMN external_ref TEXT CHECK (length(external_ref) <= 200);
CREATE INDEX meetings_integration_idx ON meetings(integration_id, created_at DESC);

-- Integration meetings are hosted by whoever the integrator mints a host token for.
ALTER TABLE meetings DROP CONSTRAINT meetings_has_host;
ALTER TABLE meetings ADD CONSTRAINT meetings_has_host
    CHECK (host_id IS NOT NULL OR host_secret_hash IS NOT NULL OR integration_id IS NOT NULL);
