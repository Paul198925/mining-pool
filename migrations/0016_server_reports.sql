-- What each enrolled server last told the pool about itself: load, disk, and the state
-- and latest log lines of its services. One row per server, replaced every minute.
CREATE TABLE server_reports (
    server_id   SMALLINT PRIMARY KEY REFERENCES stratum_instances (id) ON DELETE CASCADE,
    reported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    data        JSONB NOT NULL
);
