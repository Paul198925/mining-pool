/// Splits a block reward among miners in proportion to their share weight.
///
/// Returns `(account_id, amount)` for every miner that gets at least one unit.
/// The pool fee and any rounding dust are whatever is left of `reward`.
pub fn split_reward(reward: i64, fee_percent: f64, weights: &[(i64, f64)]) -> Vec<(i64, i64)> {
    let fee = (reward as f64 * fee_percent / 100.0).ceil() as i64;
    let distributable = (reward - fee).max(0);
    let total: f64 = weights.iter().map(|(_, w)| w).sum();
    if total <= 0.0 {
        return Vec::new();
    }

    let mut remaining = distributable;
    let mut credits = Vec::with_capacity(weights.len());
    for &(account_id, weight) in weights {
        // Rounding down, plus the cap, guarantees the pool never credits more than it earned.
        let amount = ((distributable as f64 * (weight / total)).floor() as i64).min(remaining);
        if amount > 0 {
            remaining -= amount;
            credits.push((account_id, amount));
        }
    }
    credits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sum(credits: &[(i64, i64)]) -> i64 {
        credits.iter().map(|(_, a)| a).sum()
    }

    #[test]
    fn proportional_with_fee() {
        let credits = split_reward(1_000_000, 1.0, &[(1, 3.0), (2, 1.0)]);
        assert_eq!(credits, [(1, 742_500), (2, 247_500)]);
    }

    #[test]
    fn never_distributes_more_than_reward_minus_fee() {
        let weights: Vec<(i64, f64)> = (1..=7).map(|i| (i, 1.0 / 3.0)).collect();
        for reward in [1, 7, 99, 1_000_003, 50_000_000_000_000] {
            let credits = split_reward(reward, 1.5, &weights);
            let fee = (reward as f64 * 1.5 / 100.0).ceil() as i64;
            assert!(sum(&credits) <= reward - fee, "reward {reward}");
            assert!(reward - fee - sum(&credits) < 7, "dust stays below one unit per miner");
        }
    }

    #[test]
    fn single_miner_gets_everything_but_the_fee() {
        assert_eq!(split_reward(500_000_00000000, 1.0, &[(9, 0.0000015)]), [(9, 495_000_00000000)]);
    }

    #[test]
    fn no_shares_means_no_credits() {
        assert!(split_reward(1000, 1.0, &[]).is_empty());
        assert!(split_reward(1000, 1.0, &[(1, 0.0)]).is_empty());
    }
}
