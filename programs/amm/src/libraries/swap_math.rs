use super::full_math::MulDiv;
use super::liquidity_math;
use super::sqrt_price_math;
use crate::error::ErrorCode;
use crate::states::config::FEE_RATE_DENOMINATOR_VALUE;
use anchor_lang::prelude::*;
/// Result of a swap computation
/// Contains the computed price, amounts, and fees after executing a swap calculation
#[derive(Default, Debug)]
pub struct SwapComputationResult {
    /// The price after swapping the amount in/out, not to exceed the price target
    pub sqrt_price_next_x64: u128,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_amount: u64,
}

impl SwapComputationResult {
    pub fn new(sqrt_price_next_x64: u128) -> Self {
        Self {
            sqrt_price_next_x64,
            amount_in: 0,
            amount_out: 0,
            fee_amount: 0,
        }
    }
}

/// Computes the result of swapping some amount in, or amount out, given the parameters of the swap
pub fn compute_swap(
    sqrt_price_current_x64: u128,
    sqrt_price_target_x64: u128,
    liquidity: u128,
    amount_remaining: u64,
    fee_rate: u32,
    is_base_input: bool,
    zero_for_one: bool,
    is_fee_on_input: bool,
) -> Result<SwapComputationResult> {
    let mut result = SwapComputationResult::default();
    if is_base_input {
        let amount_for_price_calc = if is_fee_on_input {
            // Fee from input: amount_remaining includes fee, so we need to deduct fee first
            amount_remaining
                .mul_div_floor(
                    (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                    u64::from(FEE_RATE_DENOMINATOR_VALUE),
                )
                .ok_or(ErrorCode::CalculateOverflow)?
        } else {
            amount_remaining
        };

        let amount_in = calculate_amount_in_range(
            sqrt_price_current_x64,
            sqrt_price_target_x64,
            liquidity,
            zero_for_one,
            is_base_input,
        )?;
        if let Some(v) = amount_in {
            result.amount_in = v;
        }

        result.sqrt_price_next_x64 =
            if amount_in.is_some() && amount_for_price_calc >= result.amount_in {
                sqrt_price_target_x64
            } else {
                sqrt_price_math::get_next_sqrt_price_from_input(
                    sqrt_price_current_x64,
                    liquidity,
                    amount_for_price_calc,
                    zero_for_one,
                )?
            };
    } else {
        // amount_remaining is the net output the user wants to receive (after fee deduction if fee is from output)
        let amount_for_price_calc = if is_fee_on_input {
            amount_remaining
        } else {
            // Fee from output: amount_remaining is net output, we need gross output for price calculation
            // gross_output = net_output / (1 - fee_rate / FEE_RATE_DENOMINATOR)
            amount_remaining
                .mul_div_ceil(
                    u64::from(FEE_RATE_DENOMINATOR_VALUE).into(),
                    (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                )
                .ok_or(ErrorCode::CalculateOverflow)?
        };

        let amount_out = calculate_amount_in_range(
            sqrt_price_current_x64,
            sqrt_price_target_x64,
            liquidity,
            zero_for_one,
            is_base_input,
        )?;
        if let Some(v) = amount_out {
            result.amount_out = v;
        }
        result.sqrt_price_next_x64 =
            if amount_out.is_some() && amount_for_price_calc >= result.amount_out {
                sqrt_price_target_x64
            } else {
                sqrt_price_math::get_next_sqrt_price_from_output(
                    sqrt_price_current_x64,
                    liquidity,
                    amount_for_price_calc,
                    zero_for_one,
                )?
            }
    }

    if zero_for_one {
        require_gte!(result.sqrt_price_next_x64, sqrt_price_target_x64);
    } else {
        require_gte!(sqrt_price_target_x64, result.sqrt_price_next_x64);
    }

    // whether we reached the max possible price for the given ticks
    let max = sqrt_price_target_x64 == result.sqrt_price_next_x64;
    // get the input / output amounts when target price is not reached
    if zero_for_one {
        // if max is reached for exact input case, entire amount_in is needed
        if !(max && is_base_input) {
            result.amount_in = liquidity_math::get_delta_amount_0_unsigned(
                result.sqrt_price_next_x64,
                sqrt_price_current_x64,
                liquidity,
                true,
            )?
        };
        // if max is reached for exact output case, entire amount_out is needed
        if !(max && !is_base_input) {
            result.amount_out = liquidity_math::get_delta_amount_1_unsigned(
                result.sqrt_price_next_x64,
                sqrt_price_current_x64,
                liquidity,
                false,
            )?;
        };
    } else {
        if !(max && is_base_input) {
            result.amount_in = liquidity_math::get_delta_amount_1_unsigned(
                sqrt_price_current_x64,
                result.sqrt_price_next_x64,
                liquidity,
                true,
            )?
        };
        if !(max && !is_base_input) {
            result.amount_out = liquidity_math::get_delta_amount_0_unsigned(
                sqrt_price_current_x64,
                result.sqrt_price_next_x64,
                liquidity,
                false,
            )?
        };
    }

    if is_base_input {
        if is_fee_on_input {
            if result.sqrt_price_next_x64 != sqrt_price_target_x64 {
                result.fee_amount = amount_remaining
                    .checked_sub(result.amount_in)
                    .ok_or(ErrorCode::CalculateOverflow)?;
            } else {
                result.fee_amount = result
                    .amount_in
                    .mul_div_ceil(
                        fee_rate.into(),
                        (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                    )
                    .ok_or(ErrorCode::CalculateOverflow)?;
            }
        } else {
            // Fee from output: result.amount_out is gross output, fee is calculated from gross output
            // fee = gross_output * fee_rate / FEE_RATE_DENOMINATOR
            result.fee_amount = result
                .amount_out
                .mul_div_ceil(fee_rate.into(), FEE_RATE_DENOMINATOR_VALUE.into())
                .ok_or(ErrorCode::CalculateOverflow)?;
            // Deduct fee from output: user receives net output
            result.amount_out = result
                .amount_out
                .checked_sub(result.fee_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }
    } else {
        if is_fee_on_input {
            // Fee from input: amount_remaining is the desired gross output
            // Cap the gross output amount to the remaining amount
            result.amount_out = result.amount_out.min(amount_remaining);
            result.fee_amount = result
                .amount_in
                .mul_div_ceil(
                    fee_rate.into(),
                    (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                )
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else {
            result.fee_amount = result
                .amount_out
                .mul_div_ceil(fee_rate.into(), FEE_RATE_DENOMINATOR_VALUE.into())
                .ok_or(ErrorCode::CalculateOverflow)?;

            // Calculate net output
            let net_output = result
                .amount_out
                .checked_sub(result.fee_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;

            // Cap net output to amount_remaining (user's desired net output)
            // If net output exceeds amount_remaining, adjust fee to cap it
            if net_output > amount_remaining {
                // Adjust fee so that net output = amount_remaining
                result.fee_amount = result
                    .amount_out
                    .checked_sub(amount_remaining)
                    .ok_or(ErrorCode::CalculateOverflow)?;
                result.amount_out = amount_remaining;
            } else {
                // Deduct fee from output: user receives net output
                result.amount_out = net_output;
            }
        }
    }

    Ok(result)
}

/// Pre calcumate amount_in or amount_out for the specified price range
/// The amount maybe overflow of u64 due to the `sqrt_price_target_x64` maybe unreasonable.
/// Therefore, this situation needs to be handled in `compute_swap_step` to recalculate the price that can be reached based on the amount.
// #[cfg(not(test))]
fn calculate_amount_in_range(
    sqrt_price_current_x64: u128,
    sqrt_price_target_x64: u128,
    liquidity: u128,
    zero_for_one: bool,
    is_base_input: bool,
) -> Result<Option<u64>> {
    let result = if is_base_input {
        if zero_for_one {
            liquidity_math::get_delta_amount_0_unsigned(
                sqrt_price_target_x64,
                sqrt_price_current_x64,
                liquidity,
                true,
            )
        } else {
            liquidity_math::get_delta_amount_1_unsigned(
                sqrt_price_current_x64,
                sqrt_price_target_x64,
                liquidity,
                true,
            )
        }
    } else {
        if zero_for_one {
            liquidity_math::get_delta_amount_1_unsigned(
                sqrt_price_target_x64,
                sqrt_price_current_x64,
                liquidity,
                false,
            )
        } else {
            liquidity_math::get_delta_amount_0_unsigned(
                sqrt_price_current_x64,
                sqrt_price_target_x64,
                liquidity,
                false,
            )
        }
    };

    match result {
        Ok(v) => Ok(Some(v)),
        Err(e) if e == ErrorCode::MaxTokenOverflow.into() => Ok(None),
        Err(_) => Err(ErrorCode::SqrtPriceLimitOverflow.into()),
    }
}

#[cfg(test)]
mod swap_math_test {
    use crate::libraries::tick_math;

    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn compute_swap_step_test(
            sqrt_price_current_x64 in tick_math::MIN_SQRT_PRICE_X64..tick_math::MAX_SQRT_PRICE_X64,
            sqrt_price_target_x64 in tick_math::MIN_SQRT_PRICE_X64..tick_math::MAX_SQRT_PRICE_X64,
            liquidity in 1..u32::MAX as u128,
            amount_remaining in 1..u64::MAX,
            fee_rate in 1..(FEE_RATE_DENOMINATOR_VALUE - 1000), // Avoid fee_rate too close to FEE_RATE_DENOMINATOR_VALUE
            is_base_input in proptest::bool::ANY,
            is_fee_on_input in proptest::bool::ANY,
        ) {
            prop_assume!(sqrt_price_current_x64 != sqrt_price_target_x64);

            // Avoid extreme price differences that could cause overflow
            let price_diff = if sqrt_price_current_x64 > sqrt_price_target_x64 {
                sqrt_price_current_x64 - sqrt_price_target_x64
            } else {
                sqrt_price_target_x64 - sqrt_price_current_x64
            };
            prop_assume!(price_diff < u128::MAX / 1000);

            // For exact output + fee_from_output, avoid cases where amount_remaining * FEE_RATE_DENOMINATOR_VALUE would overflow
            // We need: amount_remaining * FEE_RATE_DENOMINATOR_VALUE <= u64::MAX
            // So: amount_remaining <= u64::MAX / FEE_RATE_DENOMINATOR_VALUE
            if !is_base_input && !is_fee_on_input {
                prop_assume!(amount_remaining <= u64::MAX / u64::from(FEE_RATE_DENOMINATOR_VALUE));
            }

            // For exact output + fee_from_input, we need to be very conservative
            // The fee calculation is: fee = amount_in * fee_rate / (FEE_RATE_DENOMINATOR_VALUE - fee_rate)
            // To avoid overflow, we need: amount_in * fee_rate <= u64::MAX * (FEE_RATE_DENOMINATOR_VALUE - fee_rate)
            // Since amount_in can be very large for exact output swaps, we need to limit amount_remaining more strictly
            // Also limit fee_rate to avoid cases where (FEE_RATE_DENOMINATOR_VALUE - fee_rate) is too small
            if !is_base_input && is_fee_on_input {
                // Limit amount_remaining to avoid large amount_in
                prop_assume!(amount_remaining <= 1_000_000_000u64);
                // Limit fee_rate to ensure (FEE_RATE_DENOMINATOR_VALUE - fee_rate) is not too small
                prop_assume!(fee_rate <= FEE_RATE_DENOMINATOR_VALUE - 100);
            }

            let zero_for_one = sqrt_price_current_x64 > sqrt_price_target_x64;
            let swap_step = compute_swap(
                sqrt_price_current_x64,
                sqrt_price_target_x64,
                liquidity,
                amount_remaining,
                fee_rate,
                is_base_input,
                zero_for_one,
                is_fee_on_input,
            ).unwrap();

            let amount_in = swap_step.amount_in;
            let amount_out = swap_step.amount_out;
            let sqrt_price_next_x64 = swap_step.sqrt_price_next_x64;
            let fee_amount = swap_step.fee_amount;

            // amount_used represents the amount_remaining that was actually consumed
            let amount_used = if is_base_input {
                if is_fee_on_input {
                    // amount_remaining is gross input
                    amount_in + fee_amount
                } else {
                    // amount_remaining is net input
                    amount_in
                }
            } else {
                if is_fee_on_input {
                    // amount_remaining is gross output
                    amount_out
                } else {
                    // amount_remaining is net output, but we need to check if adjustment happened
                    // If net_output > amount_remaining, fee was adjusted and gross_output = amount_out + fee_amount
                    // Otherwise, gross_output = amount_out + fee_amount
                    amount_out + fee_amount
                }
            };

            if sqrt_price_next_x64 != sqrt_price_target_x64 {
                assert!(amount_used == amount_remaining,
                    "amount_used ({}) should equal amount_remaining ({}) when target not reached",
                    amount_used, amount_remaining);
            } else {
                assert!(amount_used <= amount_remaining,
                    "amount_used ({}) should be <= amount_remaining ({}) when target reached",
                    amount_used, amount_remaining);
            }
            let price_lower = sqrt_price_current_x64.min(sqrt_price_target_x64);
            let price_upper = sqrt_price_current_x64.max(sqrt_price_target_x64);
            assert!(sqrt_price_next_x64 >= price_lower);
            assert!(sqrt_price_next_x64 <= price_upper);
        }
    }

    mod fee_calculate_mode_test {
        use super::*;

        // Test all combinations of is_base_input and is_fee_on_input
        #[test]
        fn basic_fee_mode_tests() {
            let sqrt_price_current_x64 = tick_math::get_sqrt_price_at_tick(0).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let fee_rate = 2500u32;

            // Test cases: (is_base_input, is_fee_on_input, zero_for_one, amount_remaining, description)
            let test_cases = vec![
                (
                    true,
                    true,
                    true,
                    1_000_000u64,
                    "exact_input_fee_from_input_zero_for_one",
                ),
                (
                    true,
                    true,
                    false,
                    1_000_000u64,
                    "exact_input_fee_from_input_one_for_zero",
                ),
                (
                    true,
                    false,
                    true,
                    1_000_000u64,
                    "exact_input_fee_from_output_zero_for_one",
                ),
                (
                    true,
                    false,
                    false,
                    1_000_000u64,
                    "exact_input_fee_from_output_one_for_zero",
                ),
                (
                    false,
                    true,
                    true,
                    500_000u64,
                    "exact_output_fee_from_input_zero_for_one",
                ),
                (
                    false,
                    true,
                    false,
                    500_000u64,
                    "exact_output_fee_from_input_one_for_zero",
                ),
                (
                    false,
                    false,
                    true,
                    500_000u64,
                    "exact_output_fee_from_output_zero_for_one",
                ),
                (
                    false,
                    false,
                    false,
                    500_000u64,
                    "exact_output_fee_from_output_one_for_zero",
                ),
            ];

            for (is_base_input, is_fee_on_input, zero_for_one, amount_remaining, _desc) in
                test_cases
            {
                // Use appropriate target price based on direction
                let sqrt_price_target_x64 = if zero_for_one {
                    tick_math::get_sqrt_price_at_tick(-100).unwrap()
                } else {
                    tick_math::get_sqrt_price_at_tick(100).unwrap()
                };

                let result = compute_swap(
                    sqrt_price_current_x64,
                    sqrt_price_target_x64,
                    liquidity,
                    amount_remaining,
                    fee_rate,
                    is_base_input,
                    zero_for_one,
                    is_fee_on_input,
                )
                .unwrap();

                if is_base_input {
                    if is_fee_on_input {
                        // Exact Input + fee from input: amount_remaining is gross input
                        let total_input = result.amount_in.checked_add(result.fee_amount).unwrap();
                        assert_eq!(
                            total_input, amount_remaining,
                            "gross input should equal amount_remaining"
                        );
                        let expected_fee = result
                            .amount_in
                            .mul_div_ceil(
                                fee_rate.into(),
                                (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                            )
                            .unwrap();
                        assert_eq!(
                            result.fee_amount, expected_fee,
                            "fee should be calculated from amount_in"
                        );
                    } else {
                        // Exact Input + fee from output: amount_remaining is net input
                        assert_eq!(
                            result.amount_in, amount_remaining,
                            "net input should equal amount_remaining"
                        );
                        let gross_output =
                            result.amount_out.checked_add(result.fee_amount).unwrap();
                        let expected_fee = gross_output
                            .mul_div_ceil(fee_rate.into(), FEE_RATE_DENOMINATOR_VALUE.into())
                            .unwrap();
                        assert_eq!(
                            result.fee_amount, expected_fee,
                            "fee should be calculated from gross output"
                        );
                    }
                } else {
                    if is_fee_on_input {
                        // Exact Output + fee from input: amount_remaining is gross output
                        assert_eq!(
                            result.amount_out, amount_remaining,
                            "gross output should equal amount_remaining"
                        );
                        let expected_fee = result
                            .amount_in
                            .mul_div_ceil(
                                fee_rate.into(),
                                (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                            )
                            .unwrap();
                        assert_eq!(
                            result.fee_amount, expected_fee,
                            "fee should be calculated from amount_in"
                        );
                    } else {
                        // Exact Output + fee from output: amount_remaining is net output
                        assert_eq!(
                            result.amount_out, amount_remaining,
                            "net output should equal amount_remaining"
                        );
                        let gross_output =
                            result.amount_out.checked_add(result.fee_amount).unwrap();
                        let expected_fee = gross_output
                            .mul_div_ceil(fee_rate.into(), FEE_RATE_DENOMINATOR_VALUE.into())
                            .unwrap();
                        assert_eq!(
                            result.fee_amount, expected_fee,
                            "fee should be calculated from gross output"
                        );
                    }
                }

                // Common assertions
                assert!(result.amount_in > 0, "amount_in should be positive");
                assert!(result.amount_out > 0, "amount_out should be positive");
                assert!(result.fee_amount > 0, "fee_amount should be positive");
            }
        }

        #[test]
        fn compare_fee_from_input_vs_output_exact_input() {
            // Compare fee_from_input vs fee_from_output for exact input swap
            // Test both zero_for_one directions
            let test_cases = vec![
                (
                    true,
                    tick_math::get_sqrt_price_at_tick(0).unwrap(),
                    tick_math::get_sqrt_price_at_tick(-100).unwrap(),
                ),
                (
                    false,
                    tick_math::get_sqrt_price_at_tick(0).unwrap(),
                    tick_math::get_sqrt_price_at_tick(100).unwrap(),
                ),
            ];

            for (zero_for_one, sqrt_price_current_x64, sqrt_price_target_x64) in test_cases {
                let liquidity = 1_000_000_000_000u128;
                let amount_remaining = 1_000_000u64;
                let fee_rate = 2500u32;

                // Test 1: fee from input
                let result1 = compute_swap(
                    sqrt_price_current_x64,
                    sqrt_price_target_x64,
                    liquidity,
                    amount_remaining,
                    fee_rate,
                    true,
                    zero_for_one,
                    true,
                )
                .unwrap();

                // Test 2: fee from output (use net input = gross input - fee from test 1)
                // For fair comparison, use the same net input amount
                let net_input = result1.amount_in;
                let result2 = compute_swap(
                    sqrt_price_current_x64,
                    sqrt_price_target_x64,
                    liquidity,
                    net_input,
                    fee_rate,
                    true,
                    zero_for_one,
                    false,
                )
                .unwrap();

                // Verify: same net input should produce same amount_in
                assert_eq!(
                    result1.amount_in, result2.amount_in,
                    "same net input should produce same amount_in"
                );

                // Verify: output with fee from output should be less (fee deducted from output)
                assert_eq!(result2.amount_out + result2.fee_amount, result1.amount_out,);

                // Verify: fees are calculated correctly
                assert!(result1.fee_amount > 0, "fee from input should be positive");
                assert!(result2.fee_amount > 0, "fee from output should be positive");
            }
        }

        #[test]
        fn compare_fee_from_input_vs_output_exact_output() {
            // Compare fee_from_input vs fee_from_output for exact output swap
            // Test both zero_for_one directions
            let test_cases = vec![
                (
                    true,
                    tick_math::get_sqrt_price_at_tick(0).unwrap(),
                    tick_math::get_sqrt_price_at_tick(-100).unwrap(),
                ),
                (
                    false,
                    tick_math::get_sqrt_price_at_tick(0).unwrap(),
                    tick_math::get_sqrt_price_at_tick(100).unwrap(),
                ),
            ];

            for (zero_for_one, sqrt_price_current_x64, sqrt_price_target_x64) in test_cases {
                let liquidity = 1_000_000_000_000u128;
                let amount_remaining = 500_000u64; // same amount_remaining for both tests
                let fee_rate = 2500u32;

                // Test 1: fee from output (amount_remaining is net output)
                let result1 = compute_swap(
                    sqrt_price_current_x64,
                    sqrt_price_target_x64,
                    liquidity,
                    amount_remaining,
                    fee_rate,
                    false,
                    zero_for_one,
                    false,
                )
                .unwrap();

                // Test 2: fee from input (amount_remaining is gross output)
                let result2 = compute_swap(
                    sqrt_price_current_x64,
                    sqrt_price_target_x64,
                    liquidity,
                    amount_remaining,
                    fee_rate,
                    false,
                    zero_for_one,
                    true,
                )
                .unwrap();

                // Verify: with fee from output, user gets amount_remaining as net output
                assert_eq!(
                    result1.amount_out, amount_remaining,
                    "net output should equal amount_remaining"
                );

                // Verify: with fee from input, user gets amount_remaining as gross output
                assert_eq!(
                    result2.amount_out, amount_remaining,
                    "gross output should equal amount_remaining"
                );

                // Verify: gross output in result1 should include the fee
                let result1_gross_output = result1.amount_out + result1.fee_amount;
                assert!(result1_gross_output >= result2.amount_out,);
                // Verify: gross input in result2 should include the fee
                let result2_gross_input = result2.amount_in + result2.fee_amount;
                assert!(result2_gross_input >= result1.amount_in,);

                // Verify: fee calculation methods
                // result1 fee is calculated from gross output
                let expected_fee1 = result1_gross_output
                    .mul_div_ceil(fee_rate.into(), FEE_RATE_DENOMINATOR_VALUE.into())
                    .unwrap();
                assert_eq!(
                    result1.fee_amount, expected_fee1,
                    "Fee from output should be calculated from gross output"
                );

                // result2 fee is calculated from input
                let expected_fee2 = result2
                    .amount_in
                    .mul_div_ceil(
                        fee_rate.into(),
                        (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                    )
                    .unwrap();
                assert_eq!(
                    result2.fee_amount, expected_fee2,
                    "Fee from input should be calculated from input"
                );

                // Verify: fees are calculated correctly
                assert!(result1.fee_amount > 0, "fee from output should be positive");
                assert!(result2.fee_amount > 0, "fee from input should be positive");
            }
        }
    }
}
