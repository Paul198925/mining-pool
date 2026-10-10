-- The stratum servers of the pool, one row per running process. Each registers
-- itself and keeps last_seen_at fresh; the website offers the live ones to miners.
CREATE TABLE stratum_instances (
    -- The instance id from the config: the first byte of every extranonce it hands out.
    id           SMALLINT PRIMARY KEY CHECK (id BETWEEN 0 AND 255),
    region       TEXT NOT NULL,
    host         TEXT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
