//! Reference prices for the internal exchange, from a public price API. Nothing is
//! traded there; the pool only needs to know what its coins are worth to each other.

use std::{collections::HashMap, env, time::Duration};

use anyhow::{Context, Result};
use pool_common::config::Coin;
use serde_json::Value;
use sqlx::PgPool;
use tracing::{info, warn};

const DEFAULT_API: &str = "https://api.coingecko.com/api/v3/simple/price";
/// A price that moved more than this since the last one we used is not trusted at once.
const MAX_JUMP: f64 = 0.15;
/// It is used after it has been seen this many times in a row.
const CONFIRMATIONS: u32 = 3;
const SAME_PRICE: f64 = 0.05;

/// Guards against a wrong price from the API: a sudden jump has to repeat before it counts.
#[derive(Default)]
pub struct Prices {
    accepted: HashMap<String, f64>,
    doubted: HashMap<String, (f64, u32)>,
}

impl Prices {
    /// Whether `price` may be used for `symbol` now.
    fn accept(&mut self, symbol: &str, price: f64) -> bool {
        if !(price.is_finite() && price > 0.0) {
            return false;
        }
        let close = |a: f64, b: f64, by: f64| (a / b - 1.0).abs() <= by;
        let trusted = match self.accepted.get(symbol) {
            None => true,
            Some(&last) if close(price, last, MAX_JUMP) => true,
            Some(_) => {
                let seen = match self.doubted.get(symbol) {
                    Some(&(doubted, count)) if close(price, doubted, SAME_PRICE) => count + 1,
                    _ => 1,
                };
                self.doubted.insert(symbol.to_owned(), (price, seen));
                seen >= CONFIRMATIONS
            }
        };
        if trusted {
            self.accepted.insert(symbol.to_owned(), price);
            self.doubted.remove(symbol);
        }
        trusted
    }

    /// Fetches the prices of all coins that have a `price_id` and stores the trusted ones.
    /// A coin whose price is missing or doubted keeps its old price, which then ages
    /// until the exchange stops using it.
    pub async fn refresh(&mut self, db: &PgPool, http: &reqwest::Client, coins: &[Coin]) -> Result<()> {
        let priced: Vec<(&str, &str)> =
            coins.iter().filter_map(|coin| Some((coin.symbol.as_str(), coin.price_id.as_deref()?))).collect();
        if priced.is_empty() {
            return Ok(());
        }
        let ids: Vec<&str> = priced.iter().map(|(_, id)| *id).collect();
        let api = env::var("POOL_PRICE_API_URL").unwrap_or_else(|_| DEFAULT_API.to_owned());
        let answer: Value = http
            .get(api)
            .query(&[("ids", ids.join(",").as_str()), ("vs_currencies", "usd")])
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .context("asking the price API")?
            .error_for_status()
            .context("price API")?
            .json()
            .await
            .context("reading the price API's answer")?;

        for (symbol, id) in priced {
            let Some(price) = answer[id]["usd"].as_f64() else {
                warn!(coin = symbol, "the price API has no price for {id}");
                continue;
            };
            let known = self.accepted.contains_key(symbol);
            if !self.accept(symbol, price) {
                warn!(coin = symbol, price, "price jumped, waiting for it to repeat before using it");
                continue;
            }
            if !known {
                info!(coin = symbol, price, "first price");
            }
            sqlx::query("UPDATE coins SET price_usd = $2, price_at = now() WHERE symbol = $1")
                .bind(symbol)
                .bind(price)
                .execute(db)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_moves_pass_and_jumps_must_repeat() {
        let mut prices = Prices::default();
        assert!(prices.accept("LTC", 60.0), "the first price is taken as it is");
        assert!(prices.accept("LTC", 66.0), "10 % up");
        assert!(!prices.accept("LTC", 6.6), "a tenth: probably an error");
        assert!(!prices.accept("LTC", 6.7));
        assert!(prices.accept("LTC", 6.65), "third time in a row: it is real");
        assert!(prices.accept("LTC", 7.0));
    }

    #[test]
    fn a_single_outlier_changes_nothing() {
        let mut prices = Prices::default();
        assert!(prices.accept("LTC", 60.0));
        assert!(!prices.accept("LTC", 600.0));
        assert!(prices.accept("LTC", 61.0));
        assert!(!prices.accept("LTC", 600.0), "counting starts again");
        assert!(!prices.accept("LTC", 0.0));
        assert!(!prices.accept("LTC", f64::NAN));
    }
}
