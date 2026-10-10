-- Party mining: miners who connect with "m=party.<name>" in the password form a
-- private group. A block found by a member is split by PPLNS among the party's
-- shares only, and party shares take no part in the public PPLNS.
CREATE TABLE parties (
    id         BIGSERIAL PRIMARY KEY,
    coin_id    INTEGER NOT NULL REFERENCES coins (id),
    -- SHA-256 of the party name. The name works like a shared password, so it is not stored.
    key_hash   TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (coin_id, key_hash)
);

ALTER TABLE shares ADD COLUMN party_id BIGINT REFERENCES parties (id);
ALTER TABLE blocks ADD COLUMN party_id BIGINT REFERENCES parties (id);
