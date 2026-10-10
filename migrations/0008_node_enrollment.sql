-- Servers that join the pool by themselves. An admin creates a row with a one-time
-- code; the installer on the new server trades the code for a node token and from
-- then on talks to the pool's node API with it. Such a server has no database access.
ALTER TABLE stratum_instances
    -- Symbols of the coins this server runs daemons for and mines.
    ADD COLUMN coins             TEXT[] NOT NULL DEFAULT '{}',
    -- SHA-256 of the secrets; the secrets themselves are never stored.
    ADD COLUMN token_hash        TEXT UNIQUE,
    ADD COLUMN enroll_code_hash  TEXT UNIQUE,
    ADD COLUMN enroll_expires_at TIMESTAMPTZ,
    ADD COLUMN enrolled_at       TIMESTAMPTZ;

-- A server that was added but has not started yet has never been seen.
ALTER TABLE stratum_instances ALTER COLUMN last_seen_at DROP NOT NULL;
ALTER TABLE stratum_instances ALTER COLUMN last_seen_at DROP DEFAULT;

-- Batches of shares and blocks a node has delivered. A node repeats a batch until it
-- gets an answer, and this is what keeps a repeated batch from being counted twice.
CREATE TABLE node_batches (
    node_id     SMALLINT NOT NULL,
    batch_id    TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (node_id, batch_id)
);
