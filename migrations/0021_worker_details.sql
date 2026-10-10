-- What a worker last logged in with, for the miner's wallet page: the mining software
-- as it names itself, the password options (a party name is blanked before it gets
-- here) and the algorithm of the port.
ALTER TABLE workers
    ADD COLUMN version  TEXT,
    ADD COLUMN password TEXT,
    ADD COLUMN algo     TEXT;
