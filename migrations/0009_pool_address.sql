-- Where a coin's block rewards go: an address of the pool's own wallet. pool-core
-- takes it from that wallet and publishes it here; nothing else writes this column.
ALTER TABLE coins ADD COLUMN pool_address TEXT;
