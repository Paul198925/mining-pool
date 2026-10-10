-- The hourly clean-up of old shares looks rows up by age alone.
CREATE INDEX shares_time ON shares (created_at);
