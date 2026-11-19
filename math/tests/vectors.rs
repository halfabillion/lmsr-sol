//! Exactness against 80-digit Decimal reference vectors (scripts/vectors.py).
//! Every buy must cost >= the exact LMSR cost and every sell must pay <= the
//! exact payout, each within the documented error budget.
use lmsr_math::*;
use serde_json::Value;

fn u(v: &Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}

#[test]
fn reference_vectors() {
    let data: Vec<Value> = serde_json::from_str(include_str!("vectors.json")).unwrap();
    assert!(data.len() >= 300);
    for (idx, c) in data.iter().enumerate() {
        let q: Vec<u128> = c["q"].as_array().unwrap().iter().map(u).collect();
        let n = q.len();
        let b = u(&c["b"]);
        let i = c["i"].as_u64().unwrap() as usize;
        let tol = cost_err_n(b, n).unwrap();

        let cost = lmsr_cost_n(&q, b).unwrap();
        assert!(cost.abs_diff(u(&c["cost"])) <= tol, "case {idx}: cost");

        let mut sum = 0u128;
        for (k, p) in c["prices"].as_array().unwrap().iter().enumerate() {
            let got = lmsr_price_n(&q, b, k).unwrap();
            assert!(got.abs_diff(u(p)) <= 16, "case {idx}: price {k}");
            sum += got;
        }
        assert!(sum <= WAD && WAD - sum <= 16 * n as u128, "case {idx}: price sum");

        let buy_amt = u(&c["buy_amt"]);
        let buy_exact = u(&c["buy_exact"]);
        let paid = lmsr_buy_cost_n(&q, b, i, buy_amt).unwrap();
        assert!(paid >= buy_exact, "case {idx}: buy under exact");
        assert!(paid - buy_exact <= 2 * tol + 1, "case {idx}: buy over budget");

        let sell_amt = u(&c["sell_amt"]);
        if sell_amt > 0 {
            let sell_exact = u(&c["sell_exact"]);
            let back = lmsr_sell_payout_n(&q, b, i, sell_amt).unwrap();
            assert!(back <= sell_exact, "case {idx}: sell over exact");
            assert!(sell_exact - back <= 2 * tol + 1, "case {idx}: sell under budget");
        }

        let b_ln_n = u(&c["b_ln_n"]);
        let ml = lmsr_max_loss(b, n).unwrap();
        assert!(ml >= b_ln_n, "case {idx}: subsidy below b ln n");
    }
}
