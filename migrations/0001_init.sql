CREATE TABLE coins (
    id         SERIAL PRIMARY KEY,
    symbol     TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    algo       TEXT NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A miner is identified by the payout address they connect with.
CREATE TABLE accounts (
    id             BIGSERIAL PRIMARY KEY,
    payout_coin_id INTEGER NOT NULL REFERENCES coins (id),
    payout_address TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (payout_coin_id, payout_address)
);

CREATE TABLE workers (
    id           BIGSERIAL PRIMARY KEY,
    account_id   BIGINT NOT NULL REFERENCES accounts (id),
    name         TEXT NOT NULL DEFAULT '',
    last_seen_at TIMESTAMPTZ,
    UNIQUE (account_id, name)
);
