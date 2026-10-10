-- An admin can hold back a coin's payouts, e.g. while its wallet node is being moved
-- or something looks wrong. Miners keep earning; their balances are paid once it is on again.
ALTER TABLE coins ADD COLUMN payouts_enabled BOOLEAN NOT NULL DEFAULT TRUE;
