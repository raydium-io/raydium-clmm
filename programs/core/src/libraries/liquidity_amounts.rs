///! Liquidity amount functions
///! Provides functions for computing liquidity amounts from token amounts and prices
///! Implements formulae 6.29 and 6.30
///
use super::big_num::U128;
use super::fixed_point_32;
use super::full_math::MulDiv;

/// Computes the amount of liquidity received for a given amount of token_0 and price range
/// Calculates ΔL = Δx (√P_upper x √P_lower)/(√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `amount_0` - The amount_0 being sent in
///
pub fn get_liquidity_for_amount_0(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    amount_0: u64,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };
    let intermediate = sqrt_ratio_a_x32
        .mul_div_floor(sqrt_ratio_b_x32, fixed_point_32::Q32)
        .unwrap();

    amount_0
        .mul_div_floor(intermediate, sqrt_ratio_b_x32 - sqrt_ratio_a_x32)
        .unwrap()
}

/// Computes the amount of liquidity received for a given amount of token_1 and price range
/// Calculates ΔL = Δy / (√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `amount_1` - The amount_1 being sent in
///
pub fn get_liquidity_for_amount_1(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    amount_1: u64,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    amount_1
        .mul_div_floor(fixed_point_32::Q32, sqrt_ratio_b_x32 - sqrt_ratio_a_x32)
        .unwrap()
}

/// Computes the maximum amount of liquidity received for a given amount of token_0, token_1, the current
/// pool prices and the prices at the tick boundaries
///
/// # Arguments
///
/// * `sqrt_ratio_x32` - A sqrt price representing the current pool prices
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `amount_0` - The amount of token_0 being sent in
/// * `amount_1` - The amount of token_1 being sent in
///
pub fn get_liquidity_for_amounts(
    sqrt_ratio_x32: u64,
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    amount_0: u64,
    amount_1: u64,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    if sqrt_ratio_x32 <= sqrt_ratio_a_x32 {
        // If P ≤ P_lower, only token_0 liquidity is active
        get_liquidity_for_amount_0(sqrt_ratio_a_x32, sqrt_ratio_b_x32, amount_0)
    } else if sqrt_ratio_x32 < sqrt_ratio_b_x32 {
        // If P_lower < P < P_upper, active liquidity is the minimum of the liquidity provided
        // by token_0 and token_1
        u64::min(
            get_liquidity_for_amount_0(sqrt_ratio_x32, sqrt_ratio_b_x32, amount_0),
            get_liquidity_for_amount_1(sqrt_ratio_a_x32, sqrt_ratio_x32, amount_1),
        )
    } else {
        // If P ≥ P_upper, only token_1 liquidity is active
        get_liquidity_for_amount_1(sqrt_ratio_a_x32, sqrt_ratio_b_x32, amount_1)
    }
}

/// Computes the amount of token_0 for a given amount of liquidity and a price range
/// Calculates Δx = ΔL (√P_upper - √P_lower) / (√P_upper x √P_lower)
///     = ΔL (1 / √P_lower -1 / √P_upper)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
///
pub fn get_amount_0_for_liquidity(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    liquidity: u64,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    // Token amount can't exceed u64
    ((U128::from(liquidity) << fixed_point_32::RESOLUTION)
        .mul_div_floor(
            U128::from(sqrt_ratio_b_x32 - sqrt_ratio_a_x32),
            U128::from(sqrt_ratio_b_x32),
        )
        .unwrap()
        / U128::from(sqrt_ratio_a_x32))
    .as_u64()
}

/// Computes the amount of token_1 for a given amount of liquidity and a price range
/// Calculates Δy = ΔL * (√P_upper - √P_lower)
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
///
pub fn get_amount_1_for_liquidity(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    liquidity: u64,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    liquidity
        .mul_div_floor(sqrt_ratio_b_x32 - sqrt_ratio_a_x32, fixed_point_32::Q32)
        .unwrap()
}

/// Computes the token_0 and token_1 value for a given amount of liquidity, the current
/// pool prices and the prices at the tick boundaries
///
/// # Arguments
///
/// * `sqrt_ratio_x32` - A sqrt price representing the current pool prices
/// * `sqrt_ratio_a_x32` - A sqrt price representing the first tick boundary
/// * `sqrt_ratio_b_x32` - A sqrt price representing the second tick boundary
/// * `liquidity` - The liquidity being valued
/// * `amount_0` - The amount of token_0
/// * `amount_1` - The amount of token_1
///
pub fn get_amounts_for_liquidity(
    sqrt_ratio_x32: u64,
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    liquidity: u64,
) -> (u64, u64) {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    if sqrt_ratio_x32 <= sqrt_ratio_a_x32 {
        // If P ≤ P_lower, active liquidity is entirely in token_0
        (
            get_amount_0_for_liquidity(sqrt_ratio_a_x32, sqrt_ratio_b_x32, liquidity),
            0,
        )
    } else if sqrt_ratio_x32 < sqrt_ratio_b_x32 {
        // If P_lower < P < P_upper, active liquidity is in token_0 and token_1
        (
            get_amount_0_for_liquidity(sqrt_ratio_x32, sqrt_ratio_b_x32, liquidity),
            get_amount_1_for_liquidity(sqrt_ratio_a_x32, sqrt_ratio_x32, liquidity),
        )
    } else {
        // If P ≥ P_upper, active liquidity is entirely in token_1
        (
            0,
            get_amount_1_for_liquidity(sqrt_ratio_a_x32, sqrt_ratio_b_x32, liquidity),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod get_liquidity_for_amounts {
        use super::*;
        use crate::libraries::test_utils::encode_price_sqrt_x32;

        #[test]
        fn amounts_for_price_inside() {
            let sqrt_price_x32 = encode_price_sqrt_x32(1, 1);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_liquidity_for_amounts(
                    sqrt_price_x32,
                    sqrt_price_a_x32,
                    sqrt_price_b_x32,
                    100,
                    200
                ),
                2148
            );
        }

        #[test]
        fn amounts_for_price_below() {
            let sqrt_price_x32 = encode_price_sqrt_x32(99, 110);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_liquidity_for_amounts(
                    sqrt_price_x32,
                    sqrt_price_a_x32,
                    sqrt_price_b_x32,
                    100,
                    200
                ),
                1048
            );
        }

        #[test]
        fn amounts_for_price_above() {
            let sqrt_price_x32 = encode_price_sqrt_x32(111, 100);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_liquidity_for_amounts(
                    sqrt_price_x32,
                    sqrt_price_a_x32,
                    sqrt_price_b_x32,
                    100,
                    200
                ),
                2097
            );
        }

        #[test]
        fn amounts_for_price_equal_to_lower_boundary() {
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_x32 = sqrt_price_a_x32;
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_liquidity_for_amounts(
                    sqrt_price_x32,
                    sqrt_price_a_x32,
                    sqrt_price_b_x32,
                    100,
                    200
                ),
                1048
            );
        }

        #[test]
        fn amounts_for_price_equal_to_upper_boundary() {
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);
            let sqrt_price_x32 = sqrt_price_b_x32;

            assert_eq!(
                get_liquidity_for_amounts(
                    sqrt_price_x32,
                    sqrt_price_a_x32,
                    sqrt_price_b_x32,
                    100,
                    200
                ),
                2097
            );
        }
    }

    mod get_amount_0_for_liquidity {
        use super::*;
        use crate::libraries::test_utils::encode_price_sqrt_x32;

        #[test]
        fn amounts_for_price_inside() {
            let sqrt_price_x32 = encode_price_sqrt_x32(1, 1);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_amounts_for_liquidity(sqrt_price_x32, sqrt_price_a_x32, sqrt_price_b_x32, 2148),
                (99, 99)
            );
        }

        #[test]
        fn amounts_for_price_below() {
            let sqrt_price_x32 = encode_price_sqrt_x32(99, 110);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_amounts_for_liquidity(sqrt_price_x32, sqrt_price_a_x32, sqrt_price_b_x32, 1048),
                (99, 0)
            );
        }

        #[test]
        fn amounts_for_price_above() {
            let sqrt_price_x32 = encode_price_sqrt_x32(111, 100);
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_amounts_for_liquidity(sqrt_price_x32, sqrt_price_a_x32, sqrt_price_b_x32, 2097),
                (0, 199)
            );
        }

        #[test]
        fn amounts_for_price_on_lower_boundary() {
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_x32 = sqrt_price_a_x32;
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);

            assert_eq!(
                get_amounts_for_liquidity(sqrt_price_x32, sqrt_price_a_x32, sqrt_price_b_x32, 1048),
                (99, 0)
            );
        }

        #[test]
        fn amounts_for_price_on_upper_boundary() {
            let sqrt_price_a_x32 = encode_price_sqrt_x32(100, 110);
            let sqrt_price_b_x32 = encode_price_sqrt_x32(110, 100);
            let sqrt_price_x32 = sqrt_price_b_x32;

            assert_eq!(
                get_amounts_for_liquidity(sqrt_price_x32, sqrt_price_a_x32, sqrt_price_b_x32, 2097),
                (0, 199)
            );
        }
    }
}
