//! WAD (1e18) fixed-point core for an N-outcome LMSR prediction-market maker.
//!
//! Port of the math in lmsr-fe (github.com/halfabillion/lmsr-fe, Apache-2.0)
//! from Fe/u256 to Rust/u128 with a 256-bit intermediate for `mul_div`.
//!
//! LMSR is evaluated in max-shifted log-sum-exp form, so `exp` only ever sees
//! non-positive arguments and `ln` only arguments >= 1. No signed fixed point.
//!
//! ```text
//! C(q)   = m + b * ln( sum_i exp(-(m - q_i) / b) ),   m = max_i q_i
//! p_i(q) = exp(-(m - q_i) / b) / sum_j exp(-(m - q_j) / b)
//! ```
//!
//! Rounding policy (maker-protective):
//! * buy  charges `max(C(q') - C(q), 0) + cost_err_n(b, n)`  (>= exact cost)
//! * sell pays    `max(C(q) - C(q') - cost_err_n(b, n), 0)`  (<= exact payout)
//! * prices are rounded down, so they sum to at most 1.0.
//!
//! All quantities (`q_i`, `b`, amounts, costs) are WAD-scaled `u128`.
#![no_std]
#![allow(clippy::manual_div_ceil)]

use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer used only as a `mul_div` intermediate.
    pub struct U256(4);
}

/// 1.0 in fixed point.
pub const WAD: u128 = 1_000_000_000_000_000_000;

/// Largest number of outcomes supported by the on-chain program.
pub const MAX_OUTCOMES: usize = 8;

/// floor(ln(2) * 1e18)
pub const LN2_WAD: u128 = 693_147_180_559_945_309;

/// floor(ln(2) * 1e36): used for k*ln2 inside `ln_wad` so the error does not grow with k.
pub const LN2_E36: u128 = 693_147_180_559_945_309_417_232_121_458_176_568;

/// Rounding-error budget per outcome per 1.0 of liquidity `b`, in wei.
pub const COST_ERR_PER_B: u128 = 64;

/// Math errors. The program maps these to `ProgramError::Custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathError {
    /// A result does not fit in u128.
    Overflow,
    /// Division by zero (e.g. b == 0).
    DivByZero,
    /// Argument outside the function's domain (e.g. ln of x < 1).
    Domain,
    /// Bad outcome index or outcome count.
    BadOutcome,
    /// Selling more shares than the book holds.
    InsufficientShares,
}

pub type MResult<T> = core::result::Result<T, MathError>;

/// floor(a * b / c) with a 256-bit intermediate.
pub fn mul_div(a: u128, b: u128, c: u128) -> MResult<u128> {
    if c == 0 {
        return Err(MathError::DivByZero);
    }
    let r = U256::from(a) * U256::from(b) / U256::from(c);
    if r > U256::from(u128::MAX) {
        return Err(MathError::Overflow);
    }
    Ok(r.as_u128())
}

/// ceil(a * b / c) with a 256-bit intermediate.
pub fn mul_div_ceil(a: u128, b: u128, c: u128) -> MResult<u128> {
    if c == 0 {
        return Err(MathError::DivByZero);
    }
    let p = U256::from(a) * U256::from(b);
    let cc = U256::from(c);
    let mut r = p / cc;
    if p % cc != U256::zero() {
        r += U256::one();
    }
    if r > U256::from(u128::MAX) {
        return Err(MathError::Overflow);
    }
    Ok(r.as_u128())
}

/// a * b / WAD rounded down.
pub fn wad_mul_down(a: u128, b: u128) -> MResult<u128> {
    mul_div(a, b, WAD)
}

/// a * b / WAD rounded up (used wherever the maker must never under-charge).
pub fn wad_mul_up(a: u128, b: u128) -> MResult<u128> {
    mul_div_ceil(a, b, WAD)
}

/// a - b, or 0 if b > a.
pub fn sat_sub(a: u128, b: u128) -> u128 {
    a.saturating_sub(b)
}

fn add(a: u128, b: u128) -> MResult<u128> {
    a.checked_add(b).ok_or(MathError::Overflow)
}

/// exp(-x) for WAD x >= 0, in WAD, rounded down (error <= a few wei).
/// Range reduction x = k*ln2 + r, 0 <= r < ln2, exp(-x) = 2^-k / exp(r).
/// Everything fits in native u128 (term * r < 2e18 * 7e17).
pub fn exp_neg_wad(x: u128) -> u128 {
    let k = x / LN2_WAD;
    if k >= 60 {
        return 0;
    }
    let r = x - k * LN2_WAD;
    let mut term = WAD;
    let mut sum = WAD;
    let mut i: u128 = 1;
    while term > 0 {
        term = term * r / (WAD * i);
        sum += term;
        i += 1;
    }
    (WAD * WAD / sum) >> k
}

/// ln(x) for WAD x >= 1.0, in WAD, rounded down (within about one wei).
/// x = 2^k * y with 1 <= y < 2, ln y = 2 atanh((y-1)/(y+1)), |z| <= 1/3,
/// series at 1e36 internal precision.
pub fn ln_wad(x: u128) -> MResult<u128> {
    if x < WAD {
        return Err(MathError::Domain);
    }
    let one: u128 = WAD * WAD; // 1e36
    // Reduce in WAD first so y * 1e18 always fits in u128.
    let mut xr = x;
    let mut k: u128 = 0;
    while xr >= 2 * WAD {
        xr >>= 1;
        k += 1;
    }
    // Low bits shifted out of xr would be lost; recover them at 1e36 precision.
    // y = x * 1e18 / 2^k computed exactly with a 256-bit intermediate.
    let y_big = (U256::from(x) * U256::from(WAD)) >> (k as usize);
    let y = y_big.as_u128();
    let z = mul_div(y - one, one, y + one)?;
    let z2 = mul_div(z, z, one)?;
    let mut term = z;
    let mut sum: u128 = 0;
    let mut i: u128 = 1;
    while term > 0 {
        sum += term / i;
        term = mul_div(term, z2, one)?;
        i += 2;
    }
    Ok((2 * sum + k * LN2_E36) / WAD)
}

/// Error budget for one n-outcome cost evaluation.
pub fn cost_err_n(b: u128, n: usize) -> MResult<u128> {
    let k = COST_ERR_PER_B * n as u128;
    add(wad_mul_up(b, k)?, k)
}

fn check_n(q: &[u128]) -> MResult<usize> {
    let n = q.len();
    if !(2..=MAX_OUTCOMES).contains(&n) {
        return Err(MathError::BadOutcome);
    }
    Ok(n)
}

/// (m, s): the max shift and sum_i exp(-(m - q_i)/b) in WAD (s in [1, n]).
fn sum_exp_n(q: &[u128], b: u128) -> MResult<(u128, u128)> {
    check_n(q)?;
    if b == 0 {
        return Err(MathError::DivByZero);
    }
    let m = *q.iter().max().unwrap();
    let mut s: u128 = 0;
    for &qi in q {
        s += exp_neg_wad(mul_div(m - qi, WAD, b)?);
    }
    Ok((m, s))
}

/// N-outcome LMSR cost C(q) with liquidity b (all WAD), rounded down.
pub fn lmsr_cost_n(q: &[u128], b: u128) -> MResult<u128> {
    let (m, s) = sum_exp_n(q, b)?;
    add(m, wad_mul_down(b, ln_wad(s)?)?)
}

/// Instantaneous price of outcome `i` (WAD probability), rounded down.
pub fn lmsr_price_n(q: &[u128], b: u128, i: usize) -> MResult<u128> {
    if i >= q.len() {
        return Err(MathError::BadOutcome);
    }
    let (m, s) = sum_exp_n(q, b)?;
    mul_div(exp_neg_wad(mul_div(m - q[i], WAD, b)?), WAD, s)
}

/// Copy `q` into a fixed buffer with outcome `i` replaced by `v`.
fn with_qi(q: &[u128], i: usize, v: u128) -> ([u128; MAX_OUTCOMES], usize) {
    let mut buf = [0u128; MAX_OUTCOMES];
    buf[..q.len()].copy_from_slice(q);
    buf[i] = v;
    (buf, q.len())
}

/// Collateral (WAD) to buy `amount` shares of outcome `i`: computed cost
/// difference plus `cost_err_n`, so it is never below the exact cost.
pub fn lmsr_buy_cost_n(q: &[u128], b: u128, i: usize, amount: u128) -> MResult<u128> {
    check_n(q)?;
    if i >= q.len() {
        return Err(MathError::BadOutcome);
    }
    let c0 = lmsr_cost_n(q, b)?;
    let (after, n) = with_qi(q, i, add(q[i], amount)?);
    let c1 = lmsr_cost_n(&after[..n], b)?;
    add(sat_sub(c1, c0), cost_err_n(b, n)?)
}

/// Collateral (WAD) paid for selling `amount` shares of outcome `i` back:
/// computed difference minus `cost_err_n`, floored at 0, so it is never above
/// the exact payout. Errors if amount > q[i].
pub fn lmsr_sell_payout_n(q: &[u128], b: u128, i: usize, amount: u128) -> MResult<u128> {
    check_n(q)?;
    if i >= q.len() {
        return Err(MathError::BadOutcome);
    }
    if amount > q[i] {
        return Err(MathError::InsufficientShares);
    }
    let c0 = lmsr_cost_n(q, b)?;
    let (after, n) = with_qi(q, i, q[i] - amount);
    let c1 = lmsr_cost_n(&after[..n], b)?;
    Ok(sat_sub(sat_sub(c0, c1), cost_err_n(b, n)?))
}

/// Subsidy (WAD) the market creator locks: b * ln(n) rounded up, plus the
/// cost error budget. With maker-protective trades the vault then always
/// covers the largest outstanding outcome.
pub fn lmsr_max_loss(b: u128, n: usize) -> MResult<u128> {
    if !(2..=MAX_OUTCOMES).contains(&n) {
        return Err(MathError::BadOutcome);
    }
    add(wad_mul_up(b, ln_wad(n as u128 * WAD)? + 1)?, cost_err_n(b, n)?)
}

/// Convert a WAD amount to token base units (`scale` = 10^(18 - decimals)),
/// rounding UP: used for what a trader pays in.
pub fn to_units_up(wad: u128, scale: u128) -> MResult<u64> {
    let u = wad / scale + u128::from(wad % scale != 0);
    u64::try_from(u).map_err(|_| MathError::Overflow)
}

/// Convert a WAD amount to token base units, rounding DOWN: used for what a
/// trader is paid out.
pub fn to_units_down(wad: u128, scale: u128) -> MResult<u64> {
    u64::try_from(wad / scale).map_err(|_| MathError::Overflow)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn exp_known_values() {
        assert_eq!(exp_neg_wad(0), WAD);
        // exp(-1) = 0.367879441171442321...
        let e1 = exp_neg_wad(WAD);
        assert!(e1.abs_diff(367_879_441_171_442_321) <= 4, "{e1}");
        // exp(-ln2) = 0.5
        assert!(exp_neg_wad(LN2_WAD).abs_diff(WAD / 2) <= 2);
        assert_eq!(exp_neg_wad(60 * WAD), 0);
    }

    #[test]
    fn ln_known_values() {
        assert_eq!(ln_wad(WAD).unwrap(), 0);
        // ln(2) = 0.693147180559945309...
        assert!(ln_wad(2 * WAD).unwrap().abs_diff(LN2_WAD) <= 1);
        // ln(3) = 1.098612288668109691...
        assert!(ln_wad(3 * WAD).unwrap().abs_diff(1_098_612_288_668_109_691) <= 1);
        // ln(8) = 2.079441541679835928...
        assert!(ln_wad(8 * WAD).unwrap().abs_diff(2_079_441_541_679_835_928) <= 1);
        assert_eq!(ln_wad(WAD - 1), Err(MathError::Domain));
    }

    #[test]
    fn flat_book_cost_is_b_ln_n() {
        let b = 100 * WAD;
        let c = lmsr_cost_n(&[0, 0, 0], b).unwrap();
        // 100 * ln 3 = 109.861228866810969139...
        assert!(c.abs_diff(109_861_228_866_810_969_139) <= cost_err_n(b, 3).unwrap());
    }

    #[test]
    fn rejects_bad_inputs() {
        assert_eq!(lmsr_cost_n(&[0], WAD), Err(MathError::BadOutcome));
        assert_eq!(lmsr_cost_n(&[0; 9], WAD), Err(MathError::BadOutcome));
        assert_eq!(lmsr_cost_n(&[0, 0], 0), Err(MathError::DivByZero));
        assert_eq!(lmsr_buy_cost_n(&[0, 0], WAD, 2, 1), Err(MathError::BadOutcome));
        assert_eq!(
            lmsr_sell_payout_n(&[5, 0], WAD, 0, 6),
            Err(MathError::InsufficientShares)
        );
    }

    #[test]
    fn unit_conversion_rounds_toward_maker() {
        let scale = 1_000_000_000_000u128; // 6-decimal mint
        assert_eq!(to_units_up(1, scale).unwrap(), 1);
        assert_eq!(to_units_down(1, scale).unwrap(), 0);
        assert_eq!(to_units_up(2 * scale, scale).unwrap(), 2);
        assert_eq!(to_units_down(2 * scale + scale - 1, scale).unwrap(), 2);
    }

    #[test]
    fn dust_trades_never_pay_out() {
        let q = [0u128, 0, 0];
        let b = 10 * WAD;
        assert!(lmsr_buy_cost_n(&q, b, 0, 1).unwrap() >= cost_err_n(b, 3).unwrap());
        let after = [1u128, 0, 0];
        assert_eq!(lmsr_sell_payout_n(&after, b, 0, 1).unwrap(), 0);
    }
}
