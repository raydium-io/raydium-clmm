//! Curve StableSwap invariant over N rate-normalised balances, used for intra-collection swaps.
use crate::libraries::big_num::U256;

const MAX_ITER: usize = 255;

fn to_u128(v: U256) -> Option<u128> {
    if v > U256::from(u128::MAX) {
        None
    } else {
        Some(v.as_u128())
    }
}

/// StableSwap D. `amp` is A (Ann = A * n).
pub fn get_d(xp: &[u128], amp: u64) -> Option<u128> {
    let n = xp.len() as u128;
    let s: u128 = xp.iter().try_fold(0u128, |a, &x| a.checked_add(x))?;
    if s == 0 {
        return Some(0);
    }
    let ann = U256::from(amp) * U256::from(n);
    let nn = U256::from(n);
    let s = U256::from(s);
    let mut d = s;
    for _ in 0..MAX_ITER {
        let mut d_p = d;
        for &x in xp {
            if x == 0 {
                return None;
            }
            d_p = d_p * d / (U256::from(x) * nn);
        }
        let d_prev = d;
        d = (ann * s + d_p * nn) * d / ((ann - U256::from(1u8)) * d + (nn + U256::from(1u8)) * d_p);
        let diff = if d > d_prev { d - d_prev } else { d_prev - d };
        if diff <= U256::from(1u8) {
            return to_u128(d);
        }
    }
    None
}

/// New balance of coin `j` after coin `i`'s balance becomes `x`, holding D.
pub fn get_y(i: usize, j: usize, x: u128, xp: &[u128], amp: u64) -> Option<u128> {
    let n = xp.len();
    let d = U256::from(get_d(xp, amp)?);
    let nn = U256::from(n as u128);
    let ann = U256::from(amp) * nn;
    let mut c = d;
    let mut s = U256::from(0u8);
    for k in 0..n {
        let xk = if k == i {
            U256::from(x)
        } else if k != j {
            U256::from(xp[k])
        } else {
            continue;
        };
        if xk.is_zero() {
            return None;
        }
        s = s + xk;
        c = c * d / (xk * nn);
    }
    c = c * d / (ann * nn);
    let b = s + d / ann;
    let mut y = d;
    for _ in 0..MAX_ITER {
        let y_prev = y;
        y = (y * y + c) / (U256::from(2u8) * y + b - d);
        let diff = if y > y_prev { y - y_prev } else { y_prev - y };
        if diff <= U256::from(1u8) {
            return to_u128(y);
        }
    }
    None
}

/// Output (normalised) for `dx` normalised units of coin `i` into coin `j`; fee must already be
/// removed from `dx`. Subtracts 1 for rounding safety, as Curve does.
pub fn swap_out(xp: &[u128], i: usize, j: usize, dx: u128, amp: u64) -> Option<u128> {
    let x_new = xp[i].checked_add(dx)?;
    let y_new = get_y(i, j, x_new, xp, amp)?;
    Some(xp[j].checked_sub(y_new)?.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peg_swap_is_near_one_to_one() {
        let xp = [1_000_000_000_000u128, 1_000_000_000_000, 1_000_000_000_000];
        let out = swap_out(&xp, 0, 1, 10_000_000_000, 200).unwrap();
        assert!(out < 10_000_000_000 && out > 9_990_000_000, "{out}");
        let mut after = xp;
        after[0] += 10_000_000_000;
        after[1] -= out;
        assert!(get_d(&after, 200).unwrap() >= get_d(&xp, 200).unwrap());
    }
    #[test]
    fn skewed_pool_bends_away_from_rate() {
        let xp = [3_000_000_000_000u128, 200_000_000_000];
        let out = swap_out(&xp, 0, 1, 100_000_000_000, 20).unwrap();
        assert!(out < 90_000_000_000, "skewed pool must not pay ~1:1: {out}");
    }
}
