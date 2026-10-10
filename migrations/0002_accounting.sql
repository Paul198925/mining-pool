-- Amounts are integers in the coin's smallest unit (1e-8 coin). Share and network
-- difficulties are in Bitcoin diff-1 units, independent of any stratum multiplier.

CREATE TABLE shares (
    id         BIGSERIAL PRIMARY KEY,
    coin_id    INTEGER NOT NULL REFERENCES coins (id),
    account_id BIGINT NOT NULL REFERENCES accounts (id),
    worker_id  BIGINT NOT NULL REFERENCES workers (id),
    difficulty DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX shares_coin_time ON shares (coin_id, created_at DESC);

CREATE TABLE blocks (
    id                 BIGSERIAL PRIMARY KEY,
    coin_id            INTEGER NOT NULL REFERENCES coins (id),
    height             BIGINT NOT NULL,
    hash               TEXT NOT NULL,
    account_id         BIGINT NOT NULL REFERENCES accounts (id),
    worker_id          BIGINT NOT NULL REFERENCES workers (id),
    reward             BIGINT NOT NULL,
    network_difficulty DOUBLE PRECISION NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'confirmed', 'orphaned')),
    confirmations      INTEGER NOT NULL DEFAULT 0,
    -- TRUE once block_credits holds this block's PPLNS split.
    credited           BOOLEAN NOT NULL DEFAULT FALSE,
    found_at           TIMESTAMPTZ NOT NULL,
    UNIQUE (coin_id, hash)
);

-- Each miner's part of a block reward. Becomes ledger entries when the block
-- matures; whatever is left of the reward is the pool fee.
CREATE TABLE block_credits (
    block_id   BIGINT NOT NULL REFERENCES blocks (id),
    account_id BIGINT NOT NULL REFERENCES accounts (id),
    amount     BIGINT NOT NULL CHECK (amount > 0),
    PRIMARY KEY (block_id, account_id)
);

CREATE SEQUENCE payout_batch_id;

CREATE TABLE payouts (
    id         BIGSERIAL PRIMARY KEY,
    coin_id    INTEGER NOT NULL REFERENCES coins (id),
    account_id BIGINT NOT NULL REFERENCES accounts (id),
    address    TEXT NOT NULL,
    amount     BIGINT NOT NULL CHECK (amount > 0),
    -- 'sending' means the wallet call was started and its outcome is not recorded yet.
    status     TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sending', 'sent')),
    batch_id   BIGINT,
    txid       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at    TIMESTAMPTZ
);
CREATE INDEX payouts_open ON payouts (coin_id, status) WHERE status <> 'sent';

-- Double-entry ledger: every transfer's entries sum to zero. Each entry belongs
-- either to a miner account or to one of the pool's own books:
--   mined       coins created by confirmed blocks (always negative)
--   pool_fee    the pool's income
--   paid_out    coins sent to miners on chain
--   network_fee transaction fees paid for payouts
CREATE SEQUENCE ledger_transfer_id;

CREATE TABLE ledger_entries (
    id             BIGSERIAL PRIMARY KEY,
    transfer_id    BIGINT NOT NULL,
    coin_id        INTEGER NOT NULL REFERENCES coins (id),
    account_id     BIGINT REFERENCES accounts (id),
    system_account TEXT CHECK (system_account IN ('mined', 'pool_fee', 'paid_out', 'network_fee')),
    amount         BIGINT NOT NULL CHECK (amount <> 0),
    kind           TEXT NOT NULL CHECK (kind IN ('block_reward', 'payout', 'network_fee')),
    block_id       BIGINT REFERENCES blocks (id),
    payout_id      BIGINT REFERENCES payouts (id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((account_id IS NULL) <> (system_account IS NULL))
);
CREATE INDEX ledger_account ON ledger_entries (account_id, coin_id) WHERE account_id IS NOT NULL;
CREATE INDEX ledger_transfer ON ledger_entries (transfer_id);

CREATE FUNCTION ledger_transfer_balanced() RETURNS trigger AS $$
BEGIN
    IF (SELECT SUM(amount) FROM ledger_entries WHERE transfer_id = NEW.transfer_id) <> 0 THEN
        RAISE EXCEPTION 'ledger transfer % does not sum to zero', NEW.transfer_id;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER ledger_balanced
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION ledger_transfer_balanced();

CREATE VIEW account_balances AS
SELECT account_id, coin_id, SUM(amount)::BIGINT AS amount
FROM ledger_entries
WHERE account_id IS NOT NULL
GROUP BY account_id, coin_id;
