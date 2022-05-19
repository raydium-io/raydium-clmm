///! Math library for liquidity
///
use crate::error::ErrorCode;
use anchor_lang::require;

/// Add a signed liquidity delta to liquidity and revert if it overflows or underflows
///
/// # Arguments
///
/// * `x` - The liquidity (L) before change
/// * `y` - The delta (ΔL) by which liquidity should be changed
///
pub fn add_delta(x: u64, y: i64) -> Result<u64, anchor_lang::error::Error> {
    let z: u64;
    if y < 0 {
        z = x - (-y as u64);
        require!(z < x, ErrorCode::LS);
    } else {
        z = x + (y as u64);
        require!(z >= x, ErrorCode::LA);
    }

    Ok(z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_plus_zero() {
        assert_eq!(add_delta(1, 0).unwrap(), 1);
    }

    #[test]
    fn one_plus_minus_one() {
        assert_eq!(add_delta(1, -1).unwrap(), 0);
    }

    #[test]
    fn one_plus_one() {
        assert_eq!(add_delta(1, 1).unwrap(), 2);
    }

    #[test]
    #[should_panic]
    fn u64_max_plus_one_overflows() {
        // gives rust overflow error in debug mode. Should give error 'LA' in release mode
        add_delta(u64::MAX, 1).unwrap();
    }

    #[test]
    #[should_panic]
    fn two_pow_64_minus_fifteen_plus_fifteen_overflows() {
        // gives rust overflow error in debug mode. Should give error 'LA' in release mode
        add_delta((u128::pow(2, 64) - 15) as u64, 15).unwrap();
    }

    #[test]
    #[should_panic]
    fn zero_minus_one_underflows() {
        // gives rust underflow error in debug mode. Should give error 'LS' in release mode
        add_delta(0, -1).unwrap();
    }

    #[test]
    #[should_panic]
    fn three_minus_four_underflows() {
        // gives rust underflow error in debug mode. Should give error 'LS' in release mode
        add_delta(3, -4).unwrap();
    }
}
