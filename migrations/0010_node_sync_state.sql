-- How far a coin's wallet node is with catching up to its network, published by
-- pool-core so the admin area can tell "still syncing" from "stuck".
ALTER TABLE coins
    ADD COLUMN headers       BIGINT,
    ADD COLUMN peers         INTEGER,
    ADD COLUMN sync_progress DOUBLE PRECISION;
