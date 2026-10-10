//! The internal exchange.
//!
//! An account belongs to the coin its payout address is on. Whatever it earns in another
//! coin (by mining that coin, or by merged mining) waits to be swapped into its own. The
//! pool never trades on an exchange: in every round the miners swap with each other at
//! a reference price, with the pool in the middle. A coin can only be handed out as far
//! as others hand it in during the same round, plus the fee income the pool holds in
//! it; what does not fit waits for a later round. The pool therefore never owes a coin
//! it does not have.

use std::collections::HashMap;

use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::info;

/// A balance is swapped once it is worth this fraction of the target coin's minimum
/// payout, so that dust does not turn into thousands of tiny bookings.
const MIN_SWAP_FRACTION: i64 = 10;

/// One account's balance in a coin that is not its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    pub account_id: i64,
    pub from: i32,
    pub to: i32,
    /// Base units of `from`.
    pub amount: i64,
}

/// What a round does with one intent. All amounts in base units.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub account_id: i64,
    pub from: i32,
    pub to: i32,
    pub from_amount: i64,
    /// What `from_amount` is worth in `to`, before the fee.
    pub to_gross: i64,
    pub fee: i64,
}

/// Decides how much of each intent is swapped in this round.
///
/// `reserves`: how much of each coin the pool may add from its own fee income.
/// `min_gross`: the smallest swap worth booking, per target coin.
pub fn settle(
    intents: &[Intent],
    prices: &HashMap<i32, f64>,
    reserves: &HashMap<i32, i64>,
    fee_percent: f64,
    min_gross: &HashMap<i32, i64>,
) -> Vec<Fill> {
    let rate = |intent: &Intent| Some(prices.get(&intent.from)? / prices.get(&intent.to)?);
    let reserve = |coin: i32| reserves.get(&coin).copied().unwrap_or(0).max(0) as f64;
    let floor_of = |coin: i32| min_gross.get(&coin).copied().unwrap_or(1).max(1);
    let mut open: Vec<(&Intent, f64)> = intents
        .iter()
        .filter(|intent| intent.amount > 0 && intent.from != intent.to)
        .filter_map(|intent| Some((intent, rate(intent).filter(|rate| rate.is_finite() && *rate > 0.0)?)))
        .filter(|(intent, rate)| (intent.amount as f64 * rate) as i64 >= floor_of(intent.to))
        .collect();
    let mut share = vec![1.0f64; open.len()];

    let fills = |open: &[(&Intent, f64)], share: &[f64]| -> Vec<Fill> {
        open.iter()
            .zip(share)
            .map(|((intent, rate), share)| {
                let from_amount = (intent.amount as f64 * share).floor() as i64;
                let to_gross = (from_amount as f64 * rate).floor() as i64;
                let fee = ((to_gross as f64 * fee_percent / 100.0).ceil() as i64).clamp(0, to_gross);
                Fill { account_id: intent.account_id, from: intent.from, to: intent.to, from_amount, to_gross, fee }
            })
            .collect()
    };
    // Per coin: how much is handed out, and how much comes in or is in reserve.
    let overdrawn = |fills: &[Fill]| -> HashMap<i32, f64> {
        let mut wanted: HashMap<i32, f64> = HashMap::new();
        let mut offered: HashMap<i32, f64> = HashMap::new();
        for fill in fills {
            *wanted.entry(fill.to).or_default() += fill.to_gross as f64;
            *offered.entry(fill.from).or_default() += fill.from_amount as f64;
        }
        wanted
            .into_iter()
            .filter_map(|(coin, wanted)| {
                let available = offered.get(&coin).copied().unwrap_or(0.0) + reserve(coin);
                (wanted > available).then_some((coin, available / wanted))
            })
            .collect()
    };

    loop {
        // Cutting the swaps into one coin reduces what their senders hand in of another, so
        // repeat until every coin is covered. Shares only ever go down, so this ends.
        for _ in 0..200 {
            let cuts = overdrawn(&fills(&open, &share));
            if cuts.is_empty() {
                break;
            }
            for ((intent, _), share) in open.iter().zip(share.iter_mut()) {
                if let Some(cut) = cuts.get(&intent.to) {
                    // A hair more than needed, so that rounding cannot leave a unit uncovered.
                    *share *= cut * (1.0 - 1e-9);
                }
            }
        }
        let result = fills(&open, &share);
        if !overdrawn(&result).is_empty() {
            // Did not settle; skip the round rather than hand out what is not there.
            return Vec::new();
        }
        let worth_it = |fill: &Fill| {
            fill.from_amount > 0 && fill.to_gross - fill.fee > 0 && fill.to_gross >= floor_of(fill.to)
        };
        if result.iter().all(worth_it) {
            return result;
        }
        // Without the swaps that came out too small the others get less: start over without them.
        let mut kept = result.iter().map(worth_it);
        open.retain(|_| kept.next().unwrap_or(false));
        share = vec![1.0; open.len()];
    }
}

async fn setting(db: &PgPool, name: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT value FROM pool_settings WHERE name = $1").bind(name).fetch_optional(db).await?)
}

/// One round of the exchange over all coins.
pub async fn run(db: &PgPool) -> Result<()> {
    if setting(db, "exchange_enabled").await?.as_deref() != Some("true") {
        return Ok(());
    }
    let fee_percent: f64 = setting(db, "exchange_fee_percent")
        .await?
        .and_then(|value| value.parse().ok())
        .filter(|fee: &f64| (0.0..=50.0).contains(fee))
        .context("exchange_fee_percent is not set to a number between 0 and 50")?;

    let mut tx = db.begin().await?;
    // Usable prices (set by hand, or fetched and fresh: better no swap than one at a stale
    // price) and, per coin, what the pool may add from its own fee income: no more
    // than that income, and no more than the wallet holds beyond what miners are owed.
    let coins: Vec<(i32, String, f64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT c.id, c.symbol, p.usd, c.min_payout,
                GREATEST(0, LEAST(
                    COALESCE((SELECT SUM(amount) FROM ledger_entries
                              WHERE coin_id = c.id AND system_account = 'pool_fee'), 0),
                    COALESCE(c.wallet_balance, 0)
                      - COALESCE((SELECT SUM(amount) FROM ledger_entries
                                  WHERE coin_id = c.id AND account_id IS NOT NULL), 0)
                      - COALESCE((SELECT SUM(amount) FROM payouts
                                  WHERE coin_id = c.id AND status <> 'sent'), 0)
                ))::BIGINT
         FROM coins c JOIN usable_prices p ON p.coin_id = c.id",
    )
    .fetch_all(&mut *tx)
    .await?;
    let prices: HashMap<i32, f64> = coins.iter().map(|coin| (coin.0, coin.2)).collect();
    let reserves: HashMap<i32, i64> = coins.iter().map(|coin| (coin.0, coin.4)).collect();
    let min_gross: HashMap<i32, i64> =
        coins.iter().map(|coin| (coin.0, coin.3.unwrap_or(0) / MIN_SWAP_FRACTION)).collect();
    let symbols: HashMap<i32, &str> = coins.iter().map(|coin| (coin.0, coin.1.as_str())).collect();

    let intents: Vec<Intent> = sqlx::query_as::<_, (i64, i32, i32, i64)>(
        "SELECT b.account_id, b.coin_id, a.payout_coin_id, b.amount
         FROM account_balances b JOIN accounts a ON a.id = b.account_id
         WHERE b.coin_id <> a.payout_coin_id AND b.amount > 0
         ORDER BY b.account_id, b.coin_id",
    )
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|(account_id, from, to, amount)| Intent { account_id, from, to, amount })
    .collect();

    let fills = settle(&intents, &prices, &reserves, fee_percent, &min_gross);
    if fills.is_empty() {
        return Ok(());
    }

    // One ledger transfer per coin, so that every coin's books balance on their own.
    let mut transfers: HashMap<i32, i64> = HashMap::new();
    let mut held: HashMap<i32, i64> = HashMap::new();
    for coin in fills.iter().flat_map(|fill| [fill.from, fill.to]) {
        if !transfers.contains_key(&coin) {
            let id: i64 = sqlx::query_scalar("SELECT nextval('ledger_transfer_id')").fetch_one(&mut *tx).await?;
            transfers.insert(coin, id);
        }
    }
    const LEG: &str = "INSERT INTO ledger_entries (transfer_id, coin_id, account_id, system_account, amount, kind, exchange_id)
                       VALUES ($1, $2, $3, $4, $5, 'exchange', $6)";
    for fill in &fills {
        let exchange_id: i64 = sqlx::query_scalar(
            "INSERT INTO exchanges (account_id, from_coin_id, to_coin_id, from_amount, to_amount, fee_amount,
                                    from_price_usd, to_price_usd)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(fill.account_id)
        .bind(fill.from)
        .bind(fill.to)
        .bind(fill.from_amount)
        .bind(fill.to_gross - fill.fee)
        .bind(fill.fee)
        .bind(prices[&fill.from])
        .bind(prices[&fill.to])
        .fetch_one(&mut *tx)
        .await?;
        // (coin, account, book, amount): the miner hands in one coin and takes out the other.
        let legs = [
            (fill.from, Some(fill.account_id), None, -fill.from_amount),
            (fill.from, None, Some("exchange"), fill.from_amount),
            (fill.to, None, Some("exchange"), -fill.to_gross),
            (fill.to, Some(fill.account_id), None, fill.to_gross - fill.fee),
            (fill.to, None, Some("pool_fee"), fill.fee),
        ];
        for (coin, account, book, amount) in legs.into_iter().filter(|leg| leg.3 != 0) {
            sqlx::query(LEG)
                .bind(transfers[&coin])
                .bind(coin)
                .bind(account)
                .bind(book)
                .bind(amount)
                .bind(exchange_id)
                .execute(&mut *tx)
                .await?;
            if book == Some("exchange") {
                *held.entry(coin).or_default() += amount;
            }
        }
    }
    // What is left in the middle is the pool's: a coin it took in for one it gave from its fee income.
    for (coin, amount) in held.into_iter().filter(|(_, amount)| *amount != 0) {
        for (book, amount) in [("exchange", -amount), ("pool_fee", amount)] {
            sqlx::query(LEG)
                .bind(transfers[&coin])
                .bind(coin)
                .bind(None::<i64>)
                .bind(book)
                .bind(amount)
                .bind(None::<i64>)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    for fill in &fills {
        let name = |coin: i32| symbols.get(&coin).copied().unwrap_or("?");
        info!(
            account = fill.account_id,
            from = name(fill.from),
            to = name(fill.to),
            gave = fill.from_amount,
            got = fill.to_gross - fill.fee,
            fee = fill.fee,
            "exchanged"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LTC: i32 = 1;
    const DOGE: i32 = 2;
    const RVN: i32 = 3;

    fn prices() -> HashMap<i32, f64> {
        HashMap::from([(LTC, 60.0), (DOGE, 0.1), (RVN, 0.002)])
    }

    fn intent(account_id: i64, from: i32, to: i32, amount: i64) -> Intent {
        Intent { account_id, from, to, amount }
    }

    fn run(intents: &[Intent], reserves: &[(i32, i64)], fee: f64) -> Vec<Fill> {
        settle(intents, &prices(), &reserves.iter().copied().collect(), fee, &HashMap::new())
    }

    /// No coin is handed out beyond what came in plus the reserve.
    fn assert_covered(fills: &[Fill], reserves: &[(i32, i64)]) {
        for coin in [LTC, DOGE, RVN] {
            let out: i64 = fills.iter().filter(|f| f.to == coin).map(|f| f.to_gross).sum();
            let came_in: i64 = fills.iter().filter(|f| f.from == coin).map(|f| f.from_amount).sum();
            let reserve = reserves.iter().find(|r| r.0 == coin).map_or(0, |r| r.1);
            assert!(out <= came_in + reserve, "coin {coin}: {out} out, {came_in} in, {reserve} reserve");
        }
    }

    #[test]
    fn nothing_happens_without_someone_going_the_other_way() {
        assert!(run(&[intent(1, DOGE, LTC, 600_00000000)], &[], 1.0).is_empty());
    }

    #[test]
    fn two_miners_swap_at_the_price_and_pay_the_fee() {
        // 600 DOGE are worth 1 LTC.
        let fills = run(&[intent(1, DOGE, LTC, 600_00000000), intent(2, LTC, DOGE, 1_00000000)], &[], 1.0);
        assert_eq!(fills.len(), 2);
        let to_ltc = fills.iter().find(|f| f.to == LTC).unwrap();
        // Rounding may cost the last unit; nothing more.
        assert!((99_999_990..=100_000_000).contains(&to_ltc.to_gross), "{to_ltc:?}");
        assert_eq!(to_ltc.fee, (to_ltc.to_gross as f64 / 100.0).ceil() as i64);
        assert_covered(&fills, &[]);
    }

    #[test]
    fn the_larger_side_is_filled_only_as_far_as_the_smaller_one_reaches() {
        let fills = run(&[intent(1, DOGE, LTC, 6000_00000000), intent(2, LTC, DOGE, 1_00000000)], &[], 0.0);
        let doge_seller = fills.iter().find(|f| f.account_id == 1).unwrap();
        assert!((599_00000000..=600_00000000).contains(&doge_seller.from_amount), "{doge_seller:?}");
        assert_covered(&fills, &[]);
    }

    #[test]
    fn several_on_one_side_share_in_proportion() {
        let intents =
            [intent(1, DOGE, LTC, 600_00000000), intent(2, DOGE, LTC, 1800_00000000), intent(3, LTC, DOGE, 2_00000000)];
        let fills = run(&intents, &[], 0.0);
        let filled = |account: i64| fills.iter().find(|f| f.account_id == account).unwrap().from_amount as f64;
        assert!((filled(2) / filled(1) - 3.0).abs() < 1e-6);
        assert!(filled(1) > 299_00000000.0 && filled(1) <= 300_00000000.0);
        assert_covered(&fills, &[]);
    }

    #[test]
    fn three_coins_can_go_round_in_a_ring() {
        // Each worth 60 dollars: nobody has a direct partner, but the ring closes.
        let intents =
            [intent(1, LTC, DOGE, 1_00000000), intent(2, DOGE, RVN, 600_00000000), intent(3, RVN, LTC, 30000_00000000)];
        let fills = run(&intents, &[], 0.5);
        assert_eq!(fills.len(), 3);
        assert!(fills.iter().all(|f| f.from_amount > 0));
        assert_covered(&fills, &[]);
    }

    #[test]
    fn the_pools_fee_income_fills_in_but_no_more_than_that() {
        let reserves = [(LTC, 50_000_000)];
        let fills = run(&[intent(1, DOGE, LTC, 600_00000000)], &reserves, 0.0);
        assert_eq!(fills.len(), 1);
        assert!((49_999_000..=50_000_000).contains(&fills[0].to_gross), "{:?}", fills[0]);
        assert_covered(&fills, &reserves);
    }

    #[test]
    fn coins_without_a_price_and_dust_are_left_alone() {
        const UNPRICED: i32 = 9;
        assert!(run(&[intent(1, UNPRICED, LTC, 1_00000000), intent(2, LTC, UNPRICED, 1_00000000)], &[], 1.0).is_empty());
        let min = HashMap::from([(LTC, 1_000_000), (DOGE, 1)]);
        let intents = [intent(1, DOGE, LTC, 1_00000000), intent(2, LTC, DOGE, 1_00000000)];
        let fills = settle(&intents, &prices(), &HashMap::new(), 1.0, &min);
        assert!(fills.is_empty(), "1 DOGE is below the smallest LTC swap, and then nothing comes back");
    }
}
