-- Solo mining: a miner who connects with "m=solo" in the password keeps the
-- whole reward of blocks they find and takes no part in PPLNS.
ALTER TABLE shares ADD COLUMN solo BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE blocks ADD COLUMN solo BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE coins ADD COLUMN solo_fee_percent DOUBLE PRECISION;

-- The part of the transaction fee that was taken out of this payout.
ALTER TABLE payouts ADD COLUMN network_fee BIGINT;
