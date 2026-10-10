-- An admin can set a coin's price by hand, e.g. for a coin the price API does not know.
ALTER TABLE coins ADD COLUMN price_manual_usd DOUBLE PRECISION CHECK (price_manual_usd > 0);

-- The price the internal exchange and the website use for a coin: the one set by hand,
-- or else the fetched one while it is fresh. No row means no usable price.
CREATE VIEW usable_prices AS
SELECT id AS coin_id,
       COALESCE(price_manual_usd, price_usd) AS usd,
       price_manual_usd IS NOT NULL AS manual
FROM coins
WHERE price_manual_usd IS NOT NULL
   OR (price_usd > 0 AND price_at > now() - interval '20 minutes');
