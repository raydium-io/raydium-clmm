use super::big_num::U128;
use super::big_num::U256;
use super::fixed_point_64;
use super::full_math::{mul_pow2_div_ceil, mul_pow2_div_floor, MulDiv};
use super::tick_math;
use crate::error::ErrorCode;
use anchor_lang::prelude::*;

/// Add a signed liquidity delta to liquidity and revert if it overflows or underflows
///
/// # Arguments
///
/// * `x` - The liquidity (L) before change
/// * `y` - The delta (ΔL) by which liquidity should be changed
///
pub fn add_delta(x: u128, y: i128) -> Result<u128> {
    let z: u128;
    if y < 0 {
        z = x - y.unsigned_abs();
        require_gt!(x, z, ErrorCode::LiquiditySubValueErr);
    } else {
        z = x + y.unsigned_abs();
        require_gte!(z, x, ErrorCode::LiquidityAddValueErr);
    }
    Ok(z)
}

/// Computes the amount of liquidity received for a given amount of token_0 and price range
/// Calculates ΔL = Δx (√P_upper x √P_lower)/(√P_upper - √P_lower)
pub fn get_liquidity_from_amount_0(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_0: u64,
) -> Result<u128> {
    if amount_0 == 0 {
        return Ok(0);
    }
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };
    let intermediate = U128::from(sqrt_ratio_a_x64)
        .mul_div_floor(
            U128::from(sqrt_ratio_b_x64),
            U128::from(fixed_point_64::Q64),
        )
        .ok_or(ErrorCode::CalculateOverflow)?;

    Ok(U128::from(amount_0)
        .mul_div_floor(
            intermediate,
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
        )
        .ok_or(ErrorCode::CalculateOverflow)?
        .as_u128())
}

/// Computes the amount of liquidity received for a given amount of token_1 and price range
/// Calculates ΔL = Δy / (√P_upper - √P_lower)
pub fn get_liquidity_from_amount_1(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_1: u64,
) -> Result<u128> {
    if amount_1 == 0 {
        return Ok(0);
    }
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    Ok(U128::from(amount_1)
        .mul_div_floor(
            U128::from(fixed_point_64::Q64),
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
        )
        .ok_or(ErrorCode::CalculateOverflow)?
        .as_u128())
}

/// Computes the maximum amount of liquidity received for a given amount of token_0, token_1, the current
/// pool prices and the prices at the tick boundaries
pub fn get_liquidity_from_amounts(
    sqrt_ratio_x64: u128,
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_0: u64,
    amount_1: u64,
) -> Result<u128> {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    if sqrt_ratio_x64 <= sqrt_ratio_a_x64 {
        // If P ≤ P_lower, only token_0 liquidity is active
        get_liquidity_from_amount_0(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_0)
    } else if sqrt_ratio_x64 < sqrt_ratio_b_x64 {
        // If P_lower < P < P_upper, active liquidity is the minimum of the liquidity provided
        // by token_0 and token_1
        Ok(u128::min(
            get_liquidity_from_amount_0(sqrt_ratio_x64, sqrt_ratio_b_x64, amount_0)?,
            get_liquidity_from_amount_1(sqrt_ratio_a_x64, sqrt_ratio_x64, amount_1)?,
        ))
    } else {
        // If P ≥ P_upper, only token_1 liquidity is active
        get_liquidity_from_amount_1(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_1)
    }
}

/// Computes the maximum amount of liquidity received for a given amount of token_0, token_1, the current
/// pool prices and the prices at the tick boundaries
pub fn get_liquidity_from_single_amount_0(
    sqrt_ratio_x64: u128,
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_0: u64,
) -> Result<u128> {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    if sqrt_ratio_x64 <= sqrt_ratio_a_x64 {
        // If P ≤ P_lower, only token_0 liquidity is active
        get_liquidity_from_amount_0(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_0)
    } else if sqrt_ratio_x64 < sqrt_ratio_b_x64 {
        // If P_lower < P < P_upper, active liquidity is the minimum of the liquidity provided
        // by token_0 and token_1
        get_liquidity_from_amount_0(sqrt_ratio_x64, sqrt_ratio_b_x64, amount_0)
    } else {
        // If P ≥ P_upper, only token_1 liquidity is active
        Ok(0)
    }
}

/// Computes the maximum amount of liquidity received for a given amount of token_0, token_1, the current
/// pool prices and the prices at the tick boundaries
pub fn get_liquidity_from_single_amount_1(
    sqrt_ratio_x64: u128,
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_1: u64,
) -> Result<u128> {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    if sqrt_ratio_x64 <= sqrt_ratio_a_x64 {
        // If P ≤ P_lower, only token_0 liquidity is active
        Ok(0)
    } else if sqrt_ratio_x64 < sqrt_ratio_b_x64 {
        // If P_lower < P < P_upper, active liquidity is the minimum of the liquidity provided
        // by token_0 and token_1
        get_liquidity_from_amount_1(sqrt_ratio_a_x64, sqrt_ratio_x64, amount_1)
    } else {
        // If P ≥ P_upper, only token_1 liquidity is active
        get_liquidity_from_amount_1(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_1)
    }
}

/// Gets the delta amount_0 for given liquidity and price range
///
/// # Formula
///
/// * `Δx = L * (1 / √P_lower - 1 / √P_upper)`
/// * i.e. `L * (√P_upper - √P_lower) / (√P_upper * √P_lower)`
pub fn get_delta_amount_0_unsigned(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u64> {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };
    require_gt!(sqrt_ratio_a_x64, 0, ErrorCode::ZeroSqrtPrice);

    let numerator_1 = U256::from(liquidity) << fixed_point_64::RESOLUTION;
    let numerator_2 = U256::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64);
    // Single `sqrt_a * sqrt_b` denominator: one U256 division instead of two.
    // Identity `floor(floor(X/b)/c) == floor(X/(b*c))` (and ceil counterpart)
    // makes this exact-equivalent to the prior two-step form.
    let denominator = U256::from(sqrt_ratio_a_x64)
        .checked_mul(U256::from(sqrt_ratio_b_x64))
        .ok_or(ErrorCode::CalculateOverflow)?;

    let result = if round_up {
        numerator_1.mul_div_ceil(numerator_2, denominator)
    } else {
        numerator_1.mul_div_floor(numerator_2, denominator)
    }
    .ok_or(ErrorCode::CalculateOverflow)?;
    if result > U256::from(u64::MAX) {
        return Err(ErrorCode::MaxTokenOverflow.into());
    }
    return Ok(result.as_u64());
}

/// Gets the delta amount_1 for given liquidity and price range
/// * `Δy = L (√P_upper - √P_lower)`
pub fn get_delta_amount_1_unsigned(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u64> {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    let result = if round_up {
        U256::from(liquidity).mul_div_ceil(
            U256::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
            U256::from(fixed_point_64::Q64),
        )
    } else {
        U256::from(liquidity).mul_div_floor(
            U256::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
            U256::from(fixed_point_64::Q64),
        )
    }
    .ok_or(ErrorCode::CalculateOverflow)?;
    if result > U256::from(u64::MAX) {
        return Err(ErrorCode::MaxTokenOverflow.into());
    }
    return Ok(result.as_u64());
}

/// Combined `(amount_in, amount_out)` for one swap step. Equivalent to a paired
/// `get_delta_amount_{0,1}_unsigned` call (input ceil, output floor) but shares
/// the `L * Δ√P` and `√P_a * √P_b` computations and replaces `* / Q64` with a
/// bit shift.
///
/// * `zero_for_one = true`:  `amount_in = ceil(amount_0)`, `amount_out = floor(amount_1)`
/// * `zero_for_one = false`: `amount_in = ceil(amount_1)`, `amount_out = floor(amount_0)`
pub fn get_delta_amounts_for_swap(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
    zero_for_one: bool,
) -> Result<(u64, u64)> {
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    }
    require_gt!(sqrt_ratio_a_x64, 0, ErrorCode::ZeroSqrtPrice);

    // Shared intermediates (exact, before rounding):
    //   amount_1_x64        = L · (sqrt_b - sqrt_a)  (= amount_1 · Q64)
    //   sqrt_price_product  = sqrt_a · sqrt_b
    // Then (same as get_delta_amount_{0,1}_unsigned, using shift/mul_pow2 helpers):
    //   amount_0 = amount_1_x64 · Q64 / sqrt_price_product   (ceil for input, floor for output)
    //   amount_1 = amount_1_x64 / Q64                         (ceil for input, floor for output)
    let sqrt_price_diff = sqrt_ratio_b_x64 - sqrt_ratio_a_x64;
    let amount_1_x64 = U256::from(liquidity)
        .checked_mul(U256::from(sqrt_price_diff))
        .ok_or(ErrorCode::CalculateOverflow)?;
    let sqrt_price_product = U256::from(sqrt_ratio_a_x64)
        .checked_mul(U256::from(sqrt_ratio_b_x64))
        .ok_or(ErrorCode::CalculateOverflow)?;

    let (amount_in_u256, amount_out_u256) = if zero_for_one {
        let amount_in = mul_pow2_div_ceil(amount_1_x64, 64, sqrt_price_product)
            .ok_or(ErrorCode::CalculateOverflow)?;
        let amount_out = amount_1_x64 >> 64;
        (amount_in, amount_out)
    } else {
        let amount_in = amount_1_x64
            .checked_add(U256::from(u64::MAX))
            .ok_or(ErrorCode::CalculateOverflow)?
            >> 64;
        let amount_out = mul_pow2_div_floor(amount_1_x64, 64, sqrt_price_product)
            .ok_or(ErrorCode::CalculateOverflow)?;
        (amount_in, amount_out)
    };

    if amount_in_u256 > U256::from(u64::MAX) {
        return Err(ErrorCode::MaxTokenOverflow.into());
    }
    if amount_out_u256 > U256::from(u64::MAX) {
        return Err(ErrorCode::MaxTokenOverflow.into());
    }
    Ok((amount_in_u256.as_u64(), amount_out_u256.as_u64()))
}

/// Helper function to get signed delta amount_0 for given liquidity and price range
pub fn get_delta_amount_0_signed(
    sqrt_ratio_a_x64: u128,
    sqrt_ratio_b_x64: u128,
    liquidity: i128,
) -> Result<u64> {
    if liquidity < 0 {
        get_delta_amount_0_unsigned(
            sqrt_ratio_a_x64,
            sqrt_ratio_b_x64,
            u128::try_from(
                liquidity
                    .checked_neg()
                    .ok_or(ErrorCode::CalculateOverflow)?,
            )
            .map_err(|_| ErrorCode::CalculateOverflow)?,
            false,
        )
    } else {
        get_delta_amount_0_unsigned(
            sqrt_ratio_a_x64,
            sqrt_ratio_b_x64,
            u128::try_from(liquidity).map_err(|_| ErrorCode::CalculateOverflow)?,
            true,
        )
    }
}

/// Helper function to get signed delta amount_1 for given liquidity and price range
pub fn get_delta_amount_1_signed(
    sqrt_ratio_a_x64: u128,
    sqrt_ratio_b_x64: u128,
    liquidity: i128,
) -> Result<u64> {
    if liquidity < 0 {
        get_delta_amount_1_unsigned(
            sqrt_ratio_a_x64,
            sqrt_ratio_b_x64,
            u128::try_from(
                liquidity
                    .checked_neg()
                    .ok_or(ErrorCode::CalculateOverflow)?,
            )
            .map_err(|_| ErrorCode::CalculateOverflow)?,
            false,
        )
    } else {
        get_delta_amount_1_unsigned(
            sqrt_ratio_a_x64,
            sqrt_ratio_b_x64,
            u128::try_from(liquidity).map_err(|_| ErrorCode::CalculateOverflow)?,
            true,
        )
    }
}

pub fn get_delta_amounts_signed(
    tick_current: i32,
    sqrt_price_x64_current: u128,
    tick_lower: i32,
    tick_upper: i32,
    liquidity_delta: i128,
) -> Result<(u64, u64)> {
    let mut amount_0 = 0;
    let mut amount_1 = 0;
    if tick_current < tick_lower {
        amount_0 = get_delta_amount_0_signed(
            tick_math::get_sqrt_price_at_tick(tick_lower)?,
            tick_math::get_sqrt_price_at_tick(tick_upper)?,
            liquidity_delta,
        )?;
    } else if tick_current < tick_upper {
        amount_0 = get_delta_amount_0_signed(
            sqrt_price_x64_current,
            tick_math::get_sqrt_price_at_tick(tick_upper)?,
            liquidity_delta,
        )?;
        amount_1 = get_delta_amount_1_signed(
            tick_math::get_sqrt_price_at_tick(tick_lower)?,
            sqrt_price_x64_current,
            liquidity_delta,
        )?;
    } else {
        amount_1 = get_delta_amount_1_signed(
            tick_math::get_sqrt_price_at_tick(tick_lower)?,
            tick_math::get_sqrt_price_at_tick(tick_upper)?,
            liquidity_delta,
        )?;
    }
    Ok((amount_0, amount_1))
}

#[cfg(test)]
mod delta_amounts_for_swap_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// `get_delta_amounts_for_swap` must match the paired
        /// `get_delta_amount_{0,1}_unsigned` calls bit-for-bit.
        #[test]
        fn combined_matches_individual_unsigned(
            sqrt_a in tick_math::MIN_SQRT_PRICE_X64..tick_math::MAX_SQRT_PRICE_X64,
            sqrt_b in tick_math::MIN_SQRT_PRICE_X64..tick_math::MAX_SQRT_PRICE_X64,
            // Full u128: production liquidity is u128, and the merged-denominator /
            // shared-product optimisation must stay bit-equivalent and fail-identically
            // up to 2^128-1, not just the 2^80 real-world band.
            liquidity in prop_oneof![1u128..(1u128 << 80), 1u128..=u128::MAX],
            zero_for_one in proptest::bool::ANY,
        ) {
            prop_assume!(sqrt_a != sqrt_b);

            let (expected_in_res, expected_out_res) = if zero_for_one {
                (
                    get_delta_amount_0_unsigned(sqrt_a, sqrt_b, liquidity, true),
                    get_delta_amount_1_unsigned(sqrt_a, sqrt_b, liquidity, false),
                )
            } else {
                (
                    get_delta_amount_1_unsigned(sqrt_a, sqrt_b, liquidity, true),
                    get_delta_amount_0_unsigned(sqrt_a, sqrt_b, liquidity, false),
                )
            };

            let combined = get_delta_amounts_for_swap(sqrt_a, sqrt_b, liquidity, zero_for_one);

            match (expected_in_res, expected_out_res, combined) {
                (Ok(expected_in), Ok(expected_out), Ok((got_in, got_out))) => {
                    assert_eq!(got_in, expected_in, "amount_in mismatch (zfo={})", zero_for_one);
                    assert_eq!(got_out, expected_out, "amount_out mismatch (zfo={})", zero_for_one);
                }
                (Err(_), _, Err(_)) | (_, Err(_), Err(_)) => {}
                (Ok(_), Ok(_), Err(e)) => {
                    panic!("combined errored where both individual succeeded: {:?}", e);
                }
                (Err(e1), _, Ok(_)) | (_, Err(e1), Ok(_)) => {
                    panic!("combined succeeded where individual errored: {:?}", e1);
                }
            }
        }
    }
}
