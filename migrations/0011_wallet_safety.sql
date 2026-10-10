-- Whether the pool wallet of a coin is encrypted and when its node last wrote a
-- backup copy, published by pool-core for the admin area.
ALTER TABLE coins
    ADD COLUMN wallet_encrypted BOOLEAN,
    ADD COLUMN wallet_backup_at TIMESTAMPTZ;
