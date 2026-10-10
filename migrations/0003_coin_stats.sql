-- What the web frontend needs to know about a coin. The services publish it
-- here so the frontend never talks to coin daemons or reads config files.
ALTER TABLE coins
    -- chain state, refreshed by pool-core
    ADD COLUMN height             BIGINT,
    ADD COLUMN network_difficulty DOUBLE PRECISION,
    ADD COLUMN network_hashrate   DOUBLE PRECISION,
    ADD COLUMN stats_at           TIMESTAMPTZ,
    -- settings, written by the services at startup
    ADD COLUMN stratum_port       INTEGER,
    ADD COLUMN block_maturity     INTEGER,
    ADD COLUMN min_payout         BIGINT,
    ADD COLUMN pool_fee_percent   DOUBLE PRECISION;

CREATE INDEX shares_account_time ON shares (account_id, created_at DESC);
