-- The pool's wallets live on enrolled servers too: for each coin one server's node runs
-- with the pool wallet, and pool-core reaches it over the private network.
ALTER TABLE stratum_instances
    -- Symbols of the coins whose pool wallet this server holds.
    ADD COLUMN wallets TEXT[] NOT NULL DEFAULT '{}',
    -- The machine pool-core runs on. It joins the network so that pool-core can reach the
    -- wallets, and wallet nodes take commands from this server only.
    ADD COLUMN core    BOOLEAN NOT NULL DEFAULT FALSE;
