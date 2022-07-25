///! Liquidity amount functions
///! Provides functions for computing liquidity amounts from token amounts and prices
///! Implements formulae 6.29 and 6.30
///
use super::big_num::U128;
use super::fixed_point_64;
use super::full_math::MulDiv;

/// Computes the amount of liquidity received for a given amount of token_0 and price range
/// Calculates ΔL = Δx (√P_upper x √P_lower)/(√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `amount_0` - The amount_0 being sent in
///
pub fn get_liquidity_for_amount_0(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_0: u64,
) -> u128 {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };
    let intermediate = U128::from(sqrt_ratio_a_x64)
        .mul_div_floor(
            U128::from(sqrt_ratio_b_x64),
            U128::from(fixed_point_64::Q64),
        )
        .unwrap();

    U128::from(amount_0)
        .mul_div_floor(
            intermediate,
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
        )
        .unwrap()
        .as_u128()
}

/// Computes the amount of liquidity received for a given amount of token_1 and price range
/// Calculates ΔL = Δy / (√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `amount_1` - The amount_1 being sent in
///
pub fn get_liquidity_for_amount_1(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_1: u64,
) -> u128 {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    U128::from(amount_1)
        .mul_div_floor(
            U128::from(fixed_point_64::Q64),
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
        )
        .unwrap()
        .as_u128()
}

/// Computes the maximum amount of liquidity received for a given amount of token_0, token_1, the current
/// pool prices and the prices at the tick boundaries
///
/// # Arguments
///
/// * `sqrt_ratio_x64` - A sqrt price representing the current pool prices
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `amount_0` - The amount of token_0 being sent in
/// * `amount_1` - The amount of token_1 being sent in
///
pub fn get_liquidity_for_amounts(
    sqrt_ratio_x64: u128,
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    amount_0: u64,
    amount_1: u64,
) -> u128 {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    if sqrt_ratio_x64 <= sqrt_ratio_a_x64 {
        // If P ≤ P_lower, only token_0 liquidity is active
        get_liquidity_for_amount_0(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_0)
    } else if sqrt_ratio_x64 < sqrt_ratio_b_x64 {
        // If P_lower < P < P_upper, active liquidity is the minimum of the liquidity provided
        // by token_0 and token_1
        u128::min(
            get_liquidity_for_amount_0(sqrt_ratio_x64, sqrt_ratio_b_x64, amount_0),
            get_liquidity_for_amount_1(sqrt_ratio_a_x64, sqrt_ratio_x64, amount_1),
        )
    } else {
        // If P ≥ P_upper, only token_1 liquidity is active
        get_liquidity_for_amount_1(sqrt_ratio_a_x64, sqrt_ratio_b_x64, amount_1)
    }
}

/// Computes the amount of token_0 for a given amount of liquidity and a price range
/// Calculates Δx = ΔL (√P_upper - √P_lower) / (√P_upper x √P_lower)
///     = ΔL (1 / √P_lower -1 / √P_upper)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
///
pub fn get_amount_0_for_liquidity(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
) -> u64 {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    // Token amount can't exceed u64
    ((U128::from(liquidity) << fixed_point_64::RESOLUTION)
        .mul_div_floor(
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
            U128::from(sqrt_ratio_b_x64),
        )
        .unwrap()
        / U128::from(sqrt_ratio_a_x64))
    .as_u64()
}

/// Computes the amount of token_1 for a given amount of liquidity and a price range
/// Calculates Δy = ΔL * (√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
///
pub fn get_amount_1_for_liquidity(
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    U128::from(liquidity)
        .mul_div_floor(
            U128::from(sqrt_ratio_b_x64 - sqrt_ratio_a_x64),
            U128::from(fixed_point_64::Q64),
        )
        .unwrap()
        .as_u64()
}

/// Computes the token_0 and token_1 value for a given amount of liquidity, the current
/// pool prices and the prices at the tick boundaries
///
/// # Arguments
///
/// * `sqrt_ratio_x64` - A sqrt price representing the current pool prices
/// * `sqrt_ratio_a_x64` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x64` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
/// * `amount_0` - The amount of token_0
/// * `amount_1` - The amount of token_1
///
pub fn get_amounts_for_liquidity(
    sqrt_ratio_x64: u128,
    mut sqrt_ratio_a_x64: u128,
    mut sqrt_ratio_b_x64: u128,
    liquidity: u128,
) -> (u64, u64) {
    // sqrt_ratio_a_x64 should hold the smaller value
    if sqrt_ratio_a_x64 > sqrt_ratio_b_x64 {
        std::mem::swap(&mut sqrt_ratio_a_x64, &mut sqrt_ratio_b_x64);
    };

    if sqrt_ratio_x64 <= sqrt_ratio_a_x64 {
        // If P ≤ P_lower, active liquidity is entirely in token_0
        (
            get_amount_0_for_liquidity(sqrt_ratio_a_x64, sqrt_ratio_b_x64, liquidity),
            0,
        )
    } else if sqrt_ratio_x64 < sqrt_ratio_b_x64 {
        // If P_lower < P < P_upper, active liquidity is in token_0 and token_1
        (
            get_amount_0_for_liquidity(sqrt_ratio_x64, sqrt_ratio_b_x64, liquidity),
            get_amount_1_for_liquidity(sqrt_ratio_a_x64, sqrt_ratio_x64, liquidity),
        )
    } else {
        // If P ≥ P_upper, active liquidity is entirely in token_1
        (
            0,
            get_amount_1_for_liquidity(sqrt_ratio_a_x64, sqrt_ratio_b_x64, liquidity),
        )
    }
}
