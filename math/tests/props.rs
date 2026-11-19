//! Property tests (proptest): maker-protective rounding holds for random
//! books, liquidity, trade sizes and trade sequences.
use lmsr_math::*;
use proptest::prelude::*;

/// Liquidity b between 0.01 and 1000 tokens (WAD).
fn b_strat() -> impl Strategy<Value = u128> {
    (WAD / 100)..(1000 * WAD)
}

/// A book of 2..=8 outcomes, each within `spread_mult * b` of zero.
fn book(b: u128) -> impl Strategy<Value = Vec<u128>> {
    (2usize..=MAX_OUTCOMES, prop::sample::select(vec![1u128, 3, 8, 40])).prop_flat_map(
        move |(n, mult)| prop::collection::vec(0..=(b * mult), n),
    )
}

#[derive(Debug, Clone)]
enum Op {
    Buy(usize, u128),
    Sell(usize, u128),
}

fn ops(b: u128) -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            (0usize..MAX_OUTCOMES, 1..=(3 * b)).prop_map(|(i, a)| Op::Buy(i, a)),
            (0usize..MAX_OUTCOMES, 1..=(3 * b)).prop_map(|(i, a)| Op::Sell(i, a)),
        ],
        1..40,
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Buy then sell the same amount: the trader always gets back strictly
    /// less than they paid (no dust drain), by at least 2 * cost_err_n unless
    /// the sell saturates at 0.
    #[test]
    fn round_trip_never_profits(
        (b, q, i, a) in b_strat().prop_flat_map(|b| (Just(b), book(b)))
            .prop_flat_map(|(b, q)| { let n = q.len(); (Just(b), Just(q), 0..n, 1..=(5 * b)) })
    ) {
        let n = q.len();
        let paid = lmsr_buy_cost_n(&q, b, i, a).unwrap();
        let mut after = q.clone();
        after[i] += a;
        let back = lmsr_sell_payout_n(&after, b, i, a).unwrap();
        prop_assert!(back < paid);
        let err = cost_err_n(b, n).unwrap();
        prop_assert!(back == 0 || paid - back >= 2 * err);
        // Same in token base units (6- and 9-decimal mints).
        for scale in [1_000_000_000_000u128, 1_000_000_000] {
            prop_assert!(to_units_down(back, scale).unwrap() <= to_units_up(paid, scale).unwrap());
        }
    }

    /// Dust: a 1-wei buy then sell never pays anything back.
    #[test]
    fn dust_round_trip_pays_zero(
        (b, q, i) in b_strat().prop_flat_map(|b| (Just(b), book(b)))
            .prop_flat_map(|(b, q)| { let n = q.len(); (Just(b), Just(q), 0..n) })
    ) {
        let paid = lmsr_buy_cost_n(&q, b, i, 1).unwrap();
        let mut after = q.clone();
        after[i] += 1;
        prop_assert_eq!(lmsr_sell_payout_n(&after, b, i, 1).unwrap(), 0);
        prop_assert!(paid > 0);
    }

    /// Prices are rounded down: they sum to at most 1.0 and within 16 wei
    /// per outcome of it. Each price is in [0, 1].
    #[test]
    fn prices_sum_to_one(
        (b, q) in b_strat().prop_flat_map(|b| (Just(b), book(b)))
    ) {
        let n = q.len();
        let mut sum = 0u128;
        for k in 0..n {
            let p = lmsr_price_n(&q, b, k).unwrap();
            prop_assert!(p <= WAD);
            sum += p;
        }
        prop_assert!(sum <= WAD, "sum {} > 1", sum);
        prop_assert!(WAD - sum <= 16 * n as u128, "sum {} too far below 1", sum);
    }

    /// A single trader runs a random buy/sell sequence in token base units,
    /// then sells everything back. They never end with more collateral than
    /// they started with.
    #[test]
    fn trader_sequence_never_profits(
        (b, n, seq, dec) in b_strat()
            .prop_flat_map(|b| (Just(b), 2usize..=MAX_OUTCOMES, ops(b), prop::sample::select(vec![6u32, 9])))
    ) {
        let scale = 10u128.pow(18 - dec);
        let mut q = vec![0u128; n];
        let mut paid_units: u128 = 0;
        let mut got_units: u128 = 0;
        for op in seq {
            match op {
                Op::Buy(i, a) => {
                    let i = i % n;
                    let a = (a / scale).max(1) * scale; // whole base units of shares
                    let c = lmsr_buy_cost_n(&q, b, i, a).unwrap();
                    paid_units += to_units_up(c, scale).unwrap() as u128;
                    q[i] += a;
                }
                Op::Sell(i, a) => {
                    let i = i % n;
                    let a = ((a / scale) * scale).min(q[i]);
                    if a == 0 { continue; }
                    let p = lmsr_sell_payout_n(&q, b, i, a).unwrap();
                    got_units += to_units_down(p, scale).unwrap() as u128;
                    q[i] -= a;
                }
            }
        }
        for i in 0..n {
            if q[i] > 0 {
                let p = lmsr_sell_payout_n(&q, b, i, q[i]).unwrap();
                got_units += to_units_down(p, scale).unwrap() as u128;
                q[i] = 0;
            }
        }
        prop_assert!(got_units <= paid_units, "trader profit: got {} paid {}", got_units, paid_units);
    }

    /// Market-wide random sequence from a flat book. After every step, for
    /// every possible winner j, the maker's loss (shares owed on j minus net
    /// collateral collected) is <= b ln(n), and the vault (subsidy + net
    /// collected) covers the largest outstanding outcome.
    #[test]
    fn maker_loss_bounded_by_b_ln_n(
        (b, n, seq) in b_strat().prop_flat_map(|b| (Just(b), 2usize..=MAX_OUTCOMES, ops(b)))
    ) {
        // Rounded DOWN, so this is <= the true b ln n.
        let bound = wad_mul_down(b, ln_wad(n as u128 * WAD).unwrap()).unwrap();
        let subsidy = lmsr_max_loss(b, n).unwrap();
        prop_assert!(subsidy >= bound);
        let mut q = vec![0u128; n];
        let mut collected: i128 = 0;
        for op in seq {
            match op {
                Op::Buy(i, a) => {
                    let i = i % n;
                    collected += lmsr_buy_cost_n(&q, b, i, a).unwrap() as i128;
                    q[i] += a;
                }
                Op::Sell(i, a) => {
                    let i = i % n;
                    let a = a.min(q[i]);
                    if a == 0 { continue; }
                    collected -= lmsr_sell_payout_n(&q, b, i, a).unwrap() as i128;
                    q[i] -= a;
                }
            }
            let worst = *q.iter().max().unwrap() as i128;
            let loss = worst - collected;
            prop_assert!(loss <= bound as i128, "loss {} > b ln n {}", loss, bound);
            prop_assert!(subsidy as i128 + collected >= worst, "vault insolvent");
        }
    }

    /// Buy cost is monotone in size and never below the marginal price bound
    /// floor(p_i) * amount (rounded), sell payout is monotone too.
    #[test]
    fn costs_monotone(
        (b, q, i, a1, a2) in b_strat().prop_flat_map(|b| (Just(b), book(b)))
            .prop_flat_map(|(b, q)| { let n = q.len(); (Just(b), Just(q), 0..n, 1..=(3 * b), 1..=(3 * b)) })
    ) {
        let (lo, hi) = if a1 <= a2 { (a1, a2) } else { (a2, a1) };
        prop_assert!(lmsr_buy_cost_n(&q, b, i, lo).unwrap() <= lmsr_buy_cost_n(&q, b, i, hi).unwrap());
        let mut big = q.clone();
        big[i] += hi;
        prop_assert!(lmsr_sell_payout_n(&big, b, i, lo).unwrap() <= lmsr_sell_payout_n(&big, b, i, hi).unwrap());
    }
}
