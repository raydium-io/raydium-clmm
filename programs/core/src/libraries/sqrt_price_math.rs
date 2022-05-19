///! Helper functions to find price changes for change in token supply and vice versa
use super::big_num::U128;
use super::fixed_point_32;
use super::full_math::MulDiv;
use super::unsafe_math::UnsafeMathTrait;

/// Gets the next sqrt price √P' given a delta of token_0
///
/// Always round up because
/// 1. In the exact output case, token 0 supply decreases leading to price increase.
/// Move price up so that exact output is met.
/// 2. In the exact input case, token 0 supply increases leading to price decrease.
/// Do not round down to minimize price impact. We only need to meet input
/// change and not guarantee exact output.
///
/// Use function for exact input or exact output swaps for token 0
///
/// # Formula
///
/// * `√P' = √P * L / (L + Δx * √P)`
/// * If Δx * √P overflows, use alternate form `√P' = L / (L/√P + Δx)`
///
/// # Proof
///
/// For constant y,
/// √P * L = y
/// √P' * L' = √P * L
/// √P' = √P * L / L'
/// √P' = √P * L / L'
/// √P' = √P * L / (L + Δx*√P)
///
/// # Arguments
///
/// * `sqrt_p_x32` - The starting price `√P`, i.e., before accounting for the token_1 delta,
/// where P is `token_1_supply / token_0_supply`
/// * `liquidity` - The amount of usable liquidity L
/// * `amount` - Delta of token 0 (Δx) to add or remove from virtual reserves
/// * `add` - Whether to add or remove the amount of token_0
///
pub fn get_next_sqrt_price_from_amount_0_rounding_up(
    sqrt_p_x32: u64,
    liquidity: u64,
    amount: u64,
    add: bool,
) -> u64 {
    // we short circuit amount == 0 because the result is otherwise not
    // guaranteed to equal the input price
    if amount == 0 {
        return sqrt_p_x32;
    };
    let numerator_1 = (U128::from(liquidity)) << fixed_point_32::RESOLUTION; // U32.32

    if add {
        // Used native overflow check instead of the `a * b / b == a` Solidity method
        // https://stackoverflow.com/q/70143451/7721443

        if let Some(product) = amount.checked_mul(sqrt_p_x32) {
            let denominator = numerator_1 + U128::from(product);
            if denominator >= numerator_1 {
                return numerator_1
                    .mul_div_ceil(U128::from(sqrt_p_x32), denominator)
                    .unwrap()
                    .as_u64();
            };
        }
        // Alternate form if overflow - `√P' = L / (L/√P + Δx)`

        U128::div_rounding_up(
            numerator_1,
            (numerator_1 / U128::from(sqrt_p_x32))
                .checked_add(U128::from(amount))
                .unwrap(),
        )
        .as_u64()
    } else {
        // if the product overflows, we know the denominator underflows
        // in addition, we must check that the denominator does not underflow
        // assert!(product / amount == sqrt_p_x32 && numerator_1 > product);
        let product = U128::from(amount.checked_mul(sqrt_p_x32).unwrap());
        assert!(numerator_1 > product);

        let denominator = numerator_1 - product;
        numerator_1
            .mul_div_ceil(U128::from(sqrt_p_x32), denominator)
            .unwrap()
            .as_u64()
    }
}

/// Gets the next sqrt price given a delta of token_1
///
/// Always round down because
/// 1. In the exact output case, token 1 supply decreases leading to price decrease.
/// Move price down by rounding down so that exact output of token 0 is met.
/// 2. In the exact input case, token 1 supply increases leading to price increase.
/// Do not round down to minimize price impact. We only need to meet input
/// change and not gurantee exact output for token 0.
///
///
/// # Formula
///
/// * `√P' = √P + Δy / L`
///
/// # Arguments
///
/// * `sqrt_p_x32` - The starting price `√P`, i.e., before accounting for the token_1 delta
/// * `liquidity` - The amount of usable liquidity L
/// * `amount` - Delta of token 1 (Δy) to add or remove from virtual reserves
/// * `add` - Whether to add or remove the amount of token_1
///
pub fn get_next_sqrt_price_from_amount_1_rounding_down(
    sqrt_p_x32: u64,
    liquidity: u64,
    amount: u64,
    add: bool,
) -> u64 {
    // if we are adding (subtracting), rounding down requires rounding the quotient down (up)
    // in both cases, avoid a mul_div for most inputs to save gas
    // if amount <= u32::MAX, overflows do not happen
    if add {
        // quotient - `Δy / L` as U32.32
        let quotient = if amount <= (u32::MAX as u64) {
            // u32::MAX or below so that amount x 2^32 does not overflow
            (amount << fixed_point_32::RESOLUTION) / liquidity
        } else {
            amount
                .mul_div_floor(fixed_point_32::Q32, liquidity as u64)
                .unwrap()
        };

        sqrt_p_x32.checked_add(quotient).unwrap()
    } else {
        let quotient = if amount <= (u32::MAX as u64) {
            u64::div_rounding_up(amount << fixed_point_32::RESOLUTION, liquidity)
        } else {
            amount.mul_div_ceil(fixed_point_32::Q32, liquidity).unwrap()
        };

        assert!(sqrt_p_x32 > quotient);
        sqrt_p_x32 - quotient
    }
}

/// Gets the next sqrt price given an input amount of token_0 or token_1
/// Throws if price or liquidity are 0, or if the next price is out of bounds
///
/// # Arguments
///
/// * `sqrt_p_x32` - The starting price `√P`, i.e., before accounting for the input amount
/// * `liquidity` - The amount of usable liquidity
/// * `amount_in` - How much of token_0, or token_1, is being swapped in
/// * `zero_for_one` - Whether the amount in is token_0 or token_1
///
pub fn get_next_sqrt_price_from_input(
    sqrt_p_x32: u64,
    liquidity: u64,
    amount_in: u64,
    zero_for_one: bool,
) -> u64 {
    assert!(sqrt_p_x32 > 0);
    assert!(liquidity > 0);

    // round to make sure that we don't pass the target price
    if zero_for_one {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_p_x32, liquidity, amount_in, true)
    } else {
        get_next_sqrt_price_from_amount_1_rounding_down(sqrt_p_x32, liquidity, amount_in, true)
    }
}

/// Gets the next sqrt price given an output amount of token0 or token1
///
/// Throws if price or liquidity are 0 or the next price is out of bounds
///
/// # Arguments
///
/// * `sqrt_p_x32` - The starting price `√P`, i.e., before accounting for the output amount
/// * `liquidity` - The amount of usable liquidity
/// * `amount_out` - How much of token_0, or token_1, is being swapped out
/// * `zero_for_one` - Whether the amount out is token_0 or token_1
///
pub fn get_next_sqrt_price_from_output(
    sqrt_p_x32: u64,
    liquidity: u64,
    amount_out: u64,
    zero_for_one: bool,
) -> u64 {
    assert!(sqrt_p_x32 > 0);
    assert!(liquidity > 0);

    if zero_for_one {
        get_next_sqrt_price_from_amount_1_rounding_down(sqrt_p_x32, liquidity, amount_out, false)
    } else {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_p_x32, liquidity, amount_out, false)
    }
}

/// Gets the amount_0 delta between two prices, for given amount of liquidity (formula 6.30)
///
/// # Formula
///
/// * `Δx = L * (1 / √P_lower - 1 / √P_upper)`
/// * i.e. `L * (√P_upper - √P_lower) / (√P_upper * √P_lower)`
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price
/// * `sqrt_ratio_b_x32` - Another sqrt price
/// * `liquidity` - The amount of usable liquidity
/// * `round_up`- Whether to round the amount up or down
///
pub fn get_amount_0_delta_unsigned(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    liquidity: u64,
    round_up: bool,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    let numerator_1 = U128::from(liquidity) << fixed_point_32::RESOLUTION;
    let numerator_2 = U128::from(sqrt_ratio_b_x32 - sqrt_ratio_a_x32);

    assert!(sqrt_ratio_a_x32 > 0);

    if round_up {
        U128::div_rounding_up(
            numerator_1
                .mul_div_ceil(numerator_2, U128::from(sqrt_ratio_b_x32))
                .unwrap(),
            U128::from(sqrt_ratio_a_x32),
        )
        .as_u64()
    } else {
        (numerator_1
            .mul_div_floor(numerator_2, U128::from(sqrt_ratio_b_x32))
            .unwrap()
            / U128::from(sqrt_ratio_a_x32))
        .as_u64()
    }
}

/// Gets the amount_1 delta between two prices, for given amount of liquidity (formula 6.30)
///
/// # Formula
///
/// * `Δy = L (√P_upper - √P_lower)`
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price
/// * `sqrt_ratio_b_x32` - Another sqrt price
/// * `liquidity` - The amount of usable liquidity
/// * `round_up`- Whether to round the amount up or down
///
pub fn get_amount_1_delta_unsigned(
    mut sqrt_ratio_a_x32: u64,
    mut sqrt_ratio_b_x32: u64,
    liquidity: u64,
    round_up: bool,
) -> u64 {
    // sqrt_ratio_a_x32 should hold the smaller value
    if sqrt_ratio_a_x32 > sqrt_ratio_b_x32 {
        std::mem::swap(&mut sqrt_ratio_a_x32, &mut sqrt_ratio_b_x32);
    };

    if round_up {
        liquidity.mul_div_ceil(sqrt_ratio_b_x32 - sqrt_ratio_a_x32, fixed_point_32::Q32)
    } else {
        liquidity.mul_div_floor(sqrt_ratio_b_x32 - sqrt_ratio_a_x32, fixed_point_32::Q32)
    }
    .unwrap()
}

/// Helper function to get signed token_0 delta between two prices,
/// for the given change in liquidity
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price
/// * `sqrt_ratio_b_x32` - Another sqrt price
/// * `liquidity` - The change in liquidity for which to compute amount_0 delta
///
pub fn get_amount_0_delta_signed(
    sqrt_ratio_a_x32: u64,
    sqrt_ratio_b_x32: u64,
    liquidity: i64,
) -> i64 {
    if liquidity < 0 {
        -(get_amount_0_delta_unsigned(sqrt_ratio_a_x32, sqrt_ratio_b_x32, -liquidity as u64, false)
            as i64)
    } else {
        // TODO check overflow, since i64::MAX < u64::MAX
        get_amount_0_delta_unsigned(sqrt_ratio_a_x32, sqrt_ratio_b_x32, liquidity as u64, true)
            as i64
    }
}

/// Helper function to get signed token_1 delta between two prices,
/// for the given change in liquidity
///
/// # Arguments
///
/// * `sqrt_ratio_a_x32` - A sqrt price
/// * `sqrt_ratio_b_x32` - Another sqrt price
/// * `liquidity` - The change in liquidity for which to compute amount_1 delta
///
pub fn get_amount_1_delta_signed(
    sqrt_ratio_a_x32: u64,
    sqrt_ratio_b_x32: u64,
    liquidity: i64,
) -> i64 {
    if liquidity < 0 {
        -(get_amount_1_delta_unsigned(sqrt_ratio_a_x32, sqrt_ratio_b_x32, -liquidity as u64, false)
            as i64)
    } else {
        // TODO check overflow, since i64::MAX < u64::MAX
        get_amount_1_delta_unsigned(sqrt_ratio_a_x32, sqrt_ratio_b_x32, liquidity as u64, true)
            as i64
    }
}

#[cfg(test)]
mod sqrt_math {
    use super::*;
    use crate::libraries::test_utils::*;

    // ---------------------------------------------------------------------
    // 1. get_next_sqrt_price_from_input()

    mod get_next_sqrt_price_from_input {
        use super::*;

        #[test]
        #[should_panic]
        fn fails_if_price_is_zero() {
            get_next_sqrt_price_from_input(0, 0, u64::pow(10, 17), false);
        }

        #[test]
        #[should_panic]
        fn fails_if_liquidity_is_zero() {
            get_next_sqrt_price_from_input(1, 0, u64::pow(10, 8), true);
        }

        #[test]
        #[should_panic]
        fn fails_if_input_amount_overflows_the_price() {
            let sqrt_p_x32 = u64::MAX;
            let liquidity: u64 = 1024;
            let amount_in: u64 = 1024;

            // sqrt_p_x32.checked_add() should fail
            get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, amount_in, false);
        }

        #[test]
        fn any_input_amount_cannot_underflow_the_price() {
            let sqrt_p_x32 = 1;
            let liquidity = 1;
            let amount_in = u64::pow(2, 63);

            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, amount_in, true),
                1
            );
        }

        #[test]
        fn returns_input_price_if_amount_in_is_zero_and_zero_for_one_is_true() {
            let sqrt_p_x32 = 1 * fixed_point_32::Q32;
            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, u64::pow(10, 8), 0, true),
                sqrt_p_x32
            );
        }

        #[test]
        fn returns_input_price_if_amount_in_is_zero_and_zero_for_one_is_false() {
            let sqrt_p_x32 = 1 * fixed_point_32::Q32;
            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, u64::pow(10, 8), 0, false),
                sqrt_p_x32
            );
        }

        #[test]
        fn returns_the_minimum_price_for_max_inputs() {
            let sqrt_p_x32 = u64::MAX - 1;
            let liquidity = u32::MAX as u64;
            let max_amount_no_overflow =
                u64::MAX - ((liquidity << fixed_point_32::RESOLUTION) / sqrt_p_x32);

            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, max_amount_no_overflow, true),
                1
            );
        }

        #[test]
        fn input_amount_of_01_token_1() {
            // price of token 0 wrt token 1 increases as token_1 supply increases
            let sqrt_p_x32 = 1 * fixed_point_32::Q32;
            let liquidity = u64::pow(10, 8);
            let amount_0_in = u64::pow(10, 7); // 10^7 / 10^8 = 0.1
            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, amount_0_in, false),
                4724464025 // `√P' = √P + Δy / L`, rounded down
                           // https://www.wolframalpha.com/input/?i=floor%282%5E32+*+%281+%2B+0.1%29%29
            );
        }

        #[test]
        fn input_amount_of_01_token_0() {
            // price of token_0 wrt token_1 decreases as token_0 supply increases
            let sqrt_p_x32 = 1 * fixed_point_32::Q32;
            let liquidity = u64::pow(10, 8);
            let amount_0_in = u64::pow(10, 7); // 10^7 / 10^8 = 0.1
            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, amount_0_in, true),
                3904515724 // `√P' = √P * L / (L + Δx * √P)`, rounded up
                           // https://www.wolframalpha.com/input/?i=ceil%282%5E32+*+%281+%2F+%281+%2B+0.1%29%29%29
            );
        }

        #[test]
        fn amount_in_is_greater_than_u32_max_and_zero_for_one_is_true() {
            let sqrt_p_x32 = 1 * fixed_point_32::Q32;
            let liquidity = u64::pow(10, 8);
            let amount_0_in = u64::pow(10, 12); // 10^12 / 10^8 = 10^4
            assert_eq!(
                get_next_sqrt_price_from_input(sqrt_p_x32, liquidity, amount_0_in, true),
                429454 // `√P' = √P * L / (L + Δx * √P)`, rounded up
                       // https://www.wolframalpha.com/input/?i=ceil%282%5E32+*+%281+%2F+%281+%2B+10%5E4%29%29%29
            );
        }

        #[test]
        fn can_return_1_with_enough_amount_in_and_zero_for_one_is_true() {
            assert_eq!(
                get_next_sqrt_price_from_input(encode_price_sqrt_x32(1, 1), 1, u64::MAX / 2, true),
                1 // `√P' = √P * L / (L + Δx * √P)`, rounded up = ceil(1 * 1 / (1 + (2^64 - 1)/2))
                  // https://www.wolframalpha.com/input/?i=ceil%281+*+1+%2F+%281+%2B+%282%5E64+-+1%29%2F2%29%29
            );
        }
    }

    // ---------------------------------------------------------------------
    // 2. get_next_sqrt_price_from_input()

    mod get_next_sqrt_price_from_output {
        use super::*;

        #[test]
        #[should_panic]
        fn fails_if_price_is_zero() {
            get_next_sqrt_price_from_output(0, 0, u64::pow(10, 17), false);
        }

        #[test]
        #[should_panic]
        fn fails_if_liquidity_is_zero() {
            get_next_sqrt_price_from_output(1, 0, u64::pow(10, 17), true);
        }

        /// Output amount should be less than virtual reserves in the pool,
        /// otherwise the function fails
        ///
        #[test]
        #[should_panic]
        fn fails_if_output_amount_is_exactly_the_virtual_reserves_of_token_0() {
            let reserve_0: u64 = 4;
            let reserve_1 = 262144;
            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 4194304
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1024
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, reserve_0, false);
        }

        /// Output amount should be less than virtual reserves in the pool,
        /// otherwise the function fails
        ///
        #[test]
        #[should_panic]
        fn fails_if_output_amount_is_greater_than_virtual_reserves_of_token_0() {
            let reserve_0: u64 = 4;
            let reserve_1 = 262144;
            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 4194304
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1024
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, reserve_0 + 1, false);
        }

        /// Output amount should be less than virtual reserves in the pool, otherwise the function fails
        #[test]
        #[should_panic]
        fn fails_if_output_amount_is_greater_than_virtual_reserves_of_token_1() {
            let reserve_0: u64 = 4;
            let reserve_1 = 262144;
            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 4194304
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1024
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, reserve_1 + 1, true);
        }

        /// Output amount should be less than virtual reserves in the pool, otherwise the function fails
        #[test]
        #[should_panic]
        fn fails_if_output_amount_is_exactly_the_virtual_reserves_of_token_1() {
            let reserve_0: u64 = 4;
            let reserve_1 = 262144;
            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 4194304
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1024
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, reserve_1, true);
        }

        #[test]
        fn succeeds_if_output_amount_is_less_than_virtual_reserves_of_token_1() {
            let reserve_0: u64 = 4;
            let reserve_1 = 262144;
            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 4194304
            println!("Sqrt p {}", sqrt_p_x32);
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1024

            assert_eq!(
                get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, reserve_1 - 1, true),
                4194304 // √P' = √P + Δy / L, rounding down = 4194304 - floor(262143 / (1024 * 2^32))
                        // https://www.wolframalpha.com/input/?i=4194304+-+floor%28262143+%2F+%281024+*+2%5E32%29%29
            );
        }

        /// If input amount is zero, there is no price movement. The input price
        /// is returned
        ///
        #[test]
        fn returns_input_price_if_amount_in_is_zero_and_zero_for_one_is_true() {
            let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
            assert_eq!(
                get_next_sqrt_price_from_output(sqrt_p_x32, u64::pow(10, 8), 0, true),
                sqrt_p_x32
            );
        }

        /// If input amount is zero, there is no price movement. The input price
        /// is returned
        ///
        #[test]
        fn returns_input_price_if_amount_in_is_zero_and_zero_for_one_is_false() {
            let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
            assert_eq!(
                get_next_sqrt_price_from_output(sqrt_p_x32, u64::pow(10, 8), 0, false),
                sqrt_p_x32
            );
        }

        #[test]
        fn output_amount_of_01_token_1_when_zero_for_one_is_false() {
            let reserve_0 = u64::pow(10, 8);
            let reserve_1 = u64::pow(10, 8);

            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 2^32 = 4294967296
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1

            let amount_0_out = reserve_1 / 10;
            assert_eq!(
                get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, amount_0_out, false),
                4772185885 // `√P' = √P * L / (L + Δx * √P)`, rounded up
                           //   = ceil(2^32 * 1 / (1 - 0.1 * 2^32 / 2^32))
                           // https://www.wolframalpha.com/input/?i=ceil%282%5E32+*+1+%2F+%281+-+0.1+*+2%5E32+%2F+2%5E32%29%29
            );
        }

        #[test]
        fn output_amount_of_01_token_1_when_zero_for_one_is_true() {
            let reserve_0 = u64::pow(10, 8);
            let reserve_1 = u64::pow(10, 8);

            let sqrt_p_x32 = encode_price_sqrt_x32(reserve_1, reserve_0); // 2^32 = 4294967296
            let liquidity = encode_liquidity(reserve_1, reserve_0); // 1

            let amount_1_out = reserve_1 / 10;

            assert_eq!(
                get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, amount_1_out, true),
                3865470566 // `√P' = √P + Δy / L`, rounded down = floor(2^32 - (0.1 / 1)*2^32)
                           // https://www.wolframalpha.com/input/?i=floor%282%5E32+-+%280.1+%2F+1%29*2%5E32%29
            );
        }

        /// Output amount should be less than virtual reserves in the pool,
        /// otherwise the function fails. Here the exact output amount is greater
        /// than liquidity available in the pool
        ///
        #[test]
        #[should_panic]
        fn reverts_if_amount_out_is_impossible_in_zero_for_one_direction() {
            let sqrt_p_x32 = encode_price_sqrt_x32(1, 1);
            let liquidity = encode_liquidity(1, 1);
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, u64::MAX, true);
        }

        /// Output amount should be less than virtual reserves in the pool,
        /// otherwise the function fails. Here the exact output amount is greater
        /// than liquidity available in the pool
        ///
        #[test]
        #[should_panic]
        fn reverts_if_amount_out_is_impossible_in_one_for_zero_direction() {
            let sqrt_p_x32 = encode_price_sqrt_x32(1, 1);
            let liquidity = encode_liquidity(1, 1);
            get_next_sqrt_price_from_output(sqrt_p_x32, liquidity, u64::MAX, false);
        }
    }

    mod get_amount_0_delta {
        use super::*;

        /// If liquidity is 0, virtual reserves are absent
        ///
        #[test]
        fn returns_0_if_liquidity_is_0() {
            assert_eq!(
                get_amount_0_delta_unsigned(
                    encode_price_sqrt_x32(1, 1),
                    encode_price_sqrt_x32(2, 1),
                    0,
                    true
                ),
                0
            )
        }

        /// Virtual reserves at a single price are constant. Price change
        /// is needed to observe a delta in reserves.
        ///
        #[test]
        fn returns_0_if_prices_are_equal() {
            assert_eq!(
                get_amount_0_delta_unsigned(
                    encode_price_sqrt_x32(100, 100),
                    encode_price_sqrt_x32(100, 100),
                    encode_liquidity(100, 100),
                    true
                ),
                0
            )
        }

        /// Returns 0.1 of amount_0 for price of 1 to 1.21
        ///
        #[test]
        fn returns_one_eleventh_of_amount_0_for_price_change_from_1_to_1_point_21() {
            let amount_0 = get_amount_0_delta_unsigned(
                encode_price_sqrt_x32(100, 100), // 2^32
                encode_price_sqrt_x32(121, 100), // 1.1 * 2^32 = 4724464026
                u64::pow(10, 8),
                true,
            );
            // Δx = L * (1 / √P_lower - 1 / √P_upper)
            //      = 10^8 (1/1 - 1/sqrt(1.21)) = 10^8 (1/1 - 1/1.1) = 10^8 * (0.1/1.1) = 10^8 / 11
            assert_eq!(amount_0, 9090910); // ceil(10^8 / 11)

            let amount_0_rounded_down = get_amount_0_delta_unsigned(
                encode_price_sqrt_x32(1, 1),
                encode_price_sqrt_x32(121, 100),
                u64::pow(10, 8),
                false,
            );
            assert_eq!(amount_0_rounded_down, amount_0 - 1); // floor(10^8 / 11)
        }

        /// Functions should handle overflow  of intermediary values without loss of precision
        ///
        /// This is ensured by the fullmath library
        ///
        /// Since `price: u64 = sqrt(reserve_1 / reserve_0) * 2^32`, it will overflow when
        /// reserve_1 > 2^32
        #[test]
        fn works_for_prices_that_overflow() {
            // Δx = L * (1 / √P_lower - 1 / √P_upper) = L (√P_upper - √P_lower)/(√P_upper * √P_lower)
            // Intermdiary value L (√P_upper - √P_lower) = 10^8 * (2^50 - 2^48) > u64::MAX, i.e overflow
            let amount_0_up = get_amount_0_delta_unsigned(
                encode_price_sqrt_x32(u64::pow(2, 50), 1), // 2^55
                encode_price_sqrt_x32(u64::pow(2, 48), 1), // 2^53
                u64::pow(10, 8),
                true,
            );
            // Δx = L * (1 / √P_lower - 1 / √P_upper) = 10^8 (1/2^24 - 1/2^25) = 2.98
            assert_eq!(amount_0_up, 3); // ceil(2.98)

            let amount_0_down = get_amount_0_delta_unsigned(
                encode_price_sqrt_x32(u64::pow(2, 50), 1), //encodePriceSqrt(BigNumber.from(2).pow(90), 1)
                encode_price_sqrt_x32(u64::pow(2, 48), 1), //encodePriceSqrt(BigNumber.from(2).pow(96), 1)
                u64::pow(10, 8),
                false,
            );

            assert_eq!(amount_0_down, 2); // floor(2.98)
        }
    }

    mod get_amount_1_delta {
        use super::*;

        /// If liquidity is 0, virtual reserves are absent
        ///
        #[test]
        fn returns_0_if_liquidity_is_0() {
            assert_eq!(
                get_amount_1_delta_unsigned(
                    encode_price_sqrt_x32(1, 1),
                    encode_price_sqrt_x32(2, 1),
                    0,
                    true
                ),
                0
            )
        }

        /// Virtual reserves at a single price are constant. Price change
        /// is needed to observe a delta in reserves.
        ///
        #[test]
        fn returns_0_if_prices_are_equal() {
            assert_eq!(
                get_amount_1_delta_unsigned(
                    encode_price_sqrt_x32(100, 100),
                    encode_price_sqrt_x32(100, 100),
                    encode_liquidity(100, 100),
                    true
                ),
                0
            )
        }

        /// Returns 0.1 of amount_0 for price of 1 to 1.21
        ///
        #[test]
        fn returns_one_tenth_of_amount_1_for_price_change_from_1_to_1_point_21() {
            let amount_1 = get_amount_1_delta_unsigned(
                encode_price_sqrt_x32(100, 100), // 2^32
                encode_price_sqrt_x32(121, 100), // 1.1 * 2^32 = 4724464026
                u64::pow(10, 8),
                true,
            );
            // Δy = L * (√P_upper - √P_lower)
            //      = 10^8 (sqrt(1.21) - 1) = 10^8 (1.1 - 1) = 10^8 * 0.1
            assert_eq!(amount_1, u64::pow(10, 7) + 1); // ceil

            let amount_1_rounded_down = get_amount_1_delta_unsigned(
                encode_price_sqrt_x32(1, 1),
                encode_price_sqrt_x32(121, 100),
                u64::pow(10, 8),
                false,
            );
            assert_eq!(amount_1_rounded_down, u64::pow(10, 7)); // floor
        }
    }

    mod swap_computation {
        use super::*;

        /// Overflow of the product `√P_upper * √P_lower` should not lead to loss
        /// of precision when finding the value of Δx
        #[test]
        fn sqrt_p_by_sqrt_q_overflows() {
            // Δx = L * (1 / √P_lower - 1 / √P_upper) = L (√P_upper - √P_lower)/(√P_upper * √P_lower)
            // √P_upper * √P_lower should be handled without loss of precision
            let amount_0_up =
                get_amount_0_delta_unsigned(u64::MAX, u64::MAX - 1, u64::pow(10, 8), true);
            // Δx = L * (1 / √P_lower - 1 / √P_upper) = 10^8 (1/2^32 - 1/(2^32 - 1) = 0...
            assert_eq!(amount_0_up, 1); // ceil

            let amount_0_down =
                get_amount_0_delta_unsigned(u64::MAX, u64::MAX - 1, u64::pow(10, 8), false);

            assert_eq!(amount_0_down, 0); // floor
        }
    }
}
