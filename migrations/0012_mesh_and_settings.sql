-- Servers form a private WireGuard network. A server can run a stratum service,
-- coin daemons, or both; a stratum service uses any coin daemon in the network.
ALTER TABLE stratum_instances
    ADD COLUMN stratum         BOOLEAN NOT NULL DEFAULT TRUE,
    -- The server's WireGuard public key; its private key never leaves it.
    ADD COLUMN wg_public_key   TEXT,
    -- Bumped whenever an admin changes what the server should run ...
    ADD COLUMN config_version  BIGINT NOT NULL DEFAULT 1,
    -- ... and what the server reported after it last applied its setup.
    ADD COLUMN applied_version BIGINT;

-- One stratum port per algorithm, editable in the admin area.
CREATE TABLE algos (
    name              TEXT PRIMARY KEY,
    port              INTEGER NOT NULL UNIQUE CHECK (port BETWEEN 1 AND 65535),
    start_difficulty  DOUBLE PRECISION NOT NULL CHECK (start_difficulty > 0),
    min_difficulty    DOUBLE PRECISION NOT NULL CHECK (min_difficulty > 0),
    max_difficulty    DOUBLE PRECISION NOT NULL CHECK (max_difficulty >= min_difficulty),
    share_target_secs DOUBLE PRECISION NOT NULL DEFAULT 15 CHECK (share_target_secs > 0)
);

INSERT INTO algos (name, port, start_difficulty, min_difficulty, max_difficulty) VALUES
    ('scrypt',         3333, 8192,   16,       100000000),
    ('neoscrypt-xaya', 3334, 4,      0.01,     1000000),
    -- RinHash miners take stratum difficulty at face value, and a CPU does some 25 kH/s per thread.
    ('rinhash',        3335, 0.0002, 0.000001, 10000);

-- Secrets the pool hands to its servers, e.g. the RPC password of the wallet-less
-- coin daemons, which stratum services on other servers need to know.
CREATE TABLE pool_secrets (
    name  TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
