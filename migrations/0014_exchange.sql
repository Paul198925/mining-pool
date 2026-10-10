-- Internal exchange: a miner can be paid in another coin than the one mined. The pool
-- swaps between its miners at a market price; it never trades on an exchange.

-- Reference prices, fetched by pool-core from a public price API.
ALTER TABLE coins
    ADD COLUMN price_usd DOUBLE PRECISION,
    ADD COLUMN price_at  TIMESTAMPTZ;

-- Settings an admin can change while the pool runs.
CREATE TABLE pool_settings (
    name  TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO pool_settings (name, value) VALUES
    ('exchange_enabled', 'true'),
    ('exchange_fee_percent', '1');

-- One row per swap of one account's balance, for the miner's and the admin's history.
CREATE TABLE exchanges (
    id             BIGSERIAL PRIMARY KEY,
    account_id     BIGINT NOT NULL REFERENCES accounts (id),
    from_coin_id   INTEGER NOT NULL REFERENCES coins (id),
    to_coin_id     INTEGER NOT NULL REFERENCES coins (id),
    -- Base units. The miner gave `from_amount` and got `to_amount`; `fee_amount` of the
    -- target coin went to the pool on top of that.
    from_amount    BIGINT NOT NULL CHECK (from_amount > 0),
    to_amount      BIGINT NOT NULL CHECK (to_amount > 0),
    fee_amount     BIGINT NOT NULL CHECK (fee_amount >= 0),
    from_price_usd DOUBLE PRECISION NOT NULL,
    to_price_usd   DOUBLE PRECISION NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX exchanges_account ON exchanges (account_id, id DESC);

-- A new book: 'exchange' holds, within one round, the coins that change hands. After
-- every round it is back at zero; what the pool itself took or gave goes to 'pool_fee'.
ALTER TABLE ledger_entries
    DROP CONSTRAINT ledger_entries_system_account_check,
    DROP CONSTRAINT ledger_entries_kind_check,
    ADD CONSTRAINT ledger_entries_system_account_check
        CHECK (system_account IN ('mined', 'pool_fee', 'paid_out', 'network_fee', 'exchange')),
    ADD CONSTRAINT ledger_entries_kind_check
        CHECK (kind IN ('block_reward', 'payout', 'network_fee', 'exchange')),
    ADD COLUMN exchange_id BIGINT REFERENCES exchanges (id);
