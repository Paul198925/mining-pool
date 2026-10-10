-- Spendable balance of the pool wallet, published by pool-core so the admin
-- area can compare it with what the pool owes its miners.
ALTER TABLE coins
    ADD COLUMN wallet_balance    BIGINT,
    ADD COLUMN wallet_checked_at TIMESTAMPTZ;
