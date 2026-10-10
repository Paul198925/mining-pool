-- Copies of the pool wallets' daily backup files, handed in by the servers that hold
-- the wallets. Encrypted wallets: the passphrases are not in the database. With them
-- here, a backup of the database takes the wallets off the server too.
CREATE TABLE wallet_backups (
    id          BIGSERIAL PRIMARY KEY,
    symbol      TEXT NOT NULL,
    server_id   SMALLINT,
    sha256      TEXT NOT NULL,
    size        INTEGER NOT NULL,
    data        BYTEA NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (symbol, sha256)
);
