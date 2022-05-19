// Helper library to find result of a swap within a single tick range, i.e. a single tick

use super::full_math::MulDiv;
use super::sqrt_price_math;

/// Result of a swap step
#[derive(Default, Debug)]
pub struct SwapStep {
    /// The price after swapping the amount in/out, not to exceed the price target
    pub sqrt_ratio_next_x32: u64,

    /// The amount to be swapped in, of either token0 or token1, based on the direction of the swap
    pub amount_in: u64,

    /// The amount to be received, of either token0 or token1, based on the direction of the swap
    pub amount_out: u64,

    /// The amount of input that will be taken as a fee
    pub fee_amount: u64,
}

/// Computes the result of swapping some amount in, or amount out, given the parameters of the swap
///
/// The fee, plus amount in, will never exceed the amount remaining if the swap's
/// `amount_specified` is positive, i.e. in an exact input swap
///
/// # Arguments
///
/// * `sqrt_ratio_current_x32` - The current sqrt price of the pool
/// * `sqrt_ratio_target_x32` - The price that cannot be exceeded, from which the direction of
/// the swap is determined
/// * `liquidity` The usable liquidity
/// * `amount_remaining` - How much input or output amount is remaining to be swapped in/out
/// * `fee_pips` - The fee taken from the input amount, expressed in hundredths of a bip (1/100 x 0.01% = 10^6)
///
pub fn compute_swap_step(
    sqrt_ratio_current_x32: u64,
    sqrt_ratio_target_x32: u64,
    liquidity: u64,
    amount_remaining: i64,
    fee_pips: u32,
) -> SwapStep {
    let zero_for_one = sqrt_ratio_current_x32 >= sqrt_ratio_target_x32;
    let exact_in = amount_remaining >= 0;
    let mut swap_step = SwapStep::default();
    if exact_in {
        // round up amount_in
        // In exact input case, amount_remaining is positive
        let amount_remaining_less_fee = (amount_remaining as u64)
            .mul_div_floor((1_000_000 - fee_pips).into(), 1_000_000)
            .unwrap();

        swap_step.amount_in = if zero_for_one {
            sqrt_price_math::get_amount_0_delta_unsigned(
                sqrt_ratio_target_x32,
                sqrt_ratio_current_x32,
                liquidity,
                true,
            )
        } else {
            sqrt_price_math::get_amount_1_delta_unsigned(
                sqrt_ratio_current_x32,
                sqrt_ratio_target_x32,
                liquidity,
                true,
            )
        };
        swap_step.sqrt_ratio_next_x32 = if amount_remaining_less_fee >= swap_step.amount_in {
            sqrt_ratio_target_x32
        } else {
            sqrt_price_math::get_next_sqrt_price_from_input(
                sqrt_ratio_current_x32,
                liquidity,
                amount_remaining_less_fee,
                zero_for_one,
            )
        };
    } else {
        // round down amount_out
        swap_step.amount_out = if zero_for_one {
            sqrt_price_math::get_amount_1_delta_unsigned(
                sqrt_ratio_target_x32,
                sqrt_ratio_current_x32,
                liquidity,
                false,
            )
        } else {
            sqrt_price_math::get_amount_0_delta_unsigned(
                sqrt_ratio_current_x32,
                sqrt_ratio_target_x32,
                liquidity,
                false,
            )
        };
        // In exact output case, amount_remaining is negative
        swap_step.sqrt_ratio_next_x32 = if (-amount_remaining as u64) >= swap_step.amount_out {
            sqrt_ratio_target_x32
        } else {
            sqrt_price_math::get_next_sqrt_price_from_output(
                sqrt_ratio_current_x32,
                liquidity,
                -amount_remaining as u64,
                zero_for_one,
            )
        }
    }

    // whether we reached the max possible price for the given ticks
    let max = sqrt_ratio_target_x32 == swap_step.sqrt_ratio_next_x32;

    // get the input / output amounts when target price is not reached
    if zero_for_one {
        // if max is reached for exact input case, entire amount_in is needed
        if !(max && exact_in) {
            swap_step.amount_in = sqrt_price_math::get_amount_0_delta_unsigned(
                swap_step.sqrt_ratio_next_x32,
                sqrt_ratio_current_x32,
                liquidity,
                true,
            )
        };
        // if max is reached for exact output case, entire amount_out is needed
        if !(max && !exact_in) {
            swap_step.amount_out = sqrt_price_math::get_amount_1_delta_unsigned(
                swap_step.sqrt_ratio_next_x32,
                sqrt_ratio_current_x32,
                liquidity,
                false,
            )
        };
    } else {
        if !(max && exact_in) {
            swap_step.amount_in = sqrt_price_math::get_amount_1_delta_unsigned(
                sqrt_ratio_current_x32,
                swap_step.sqrt_ratio_next_x32,
                liquidity,
                true,
            )
        };
        if !(max && !exact_in) {
            swap_step.amount_out = sqrt_price_math::get_amount_0_delta_unsigned(
                sqrt_ratio_current_x32,
                swap_step.sqrt_ratio_next_x32,
                liquidity,
                false,
            )
        };
    }

    // For exact output case, cap the output amount to not exceed the remaining output amount
    if !exact_in && swap_step.amount_out > (-amount_remaining as u64) {
        swap_step.amount_out = -amount_remaining as u64;
    }

    swap_step.fee_amount = if exact_in && swap_step.sqrt_ratio_next_x32 != sqrt_ratio_target_x32 {
        // we didn't reach the target, so take the remainder of the maximum input as fee
        // swap dust is granted as fee
        amount_remaining as u64 - swap_step.amount_in
    } else {
        // take pip percentage as fee
        swap_step
            .amount_in
            .mul_div_ceil(fee_pips.into(), (1_000_000 - fee_pips).into())
            .unwrap()
    };

    swap_step
}

/// Derive expected values from math formulae
///
#[cfg(test)]
mod swap_math {
    use super::*;
    use crate::libraries::test_utils::*;

    #[test]
    fn exact_amount_in_that_gets_capped_at_price_target_in_one_for_zero() {
        // exact amount in for token_1 -> token_0 swap
        // price will go up
        let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
        let sqrt_p_x32_target = encode_price_sqrt_x32(101, 100); // 4316388712

        let liquidity = 2 * u64::pow(10, 8);
        let amount = i64::pow(10, 8);
        let fee = 600;
        let zero_for_one = false;

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(sqrt_p_x32, sqrt_p_x32_target, liquidity, amount, fee);

        // √P' = √P + Δy / L = 4294967296 + (10^8 / (2 * 10^8)) * 2^32 = 6442450944
        // But we are capped at price_target = 4316388712
        let price_after_whole_input_amount = sqrt_price_math::get_next_sqrt_price_from_input(
            sqrt_p_x32,
            liquidity,
            amount as u64,
            zero_for_one,
        );
        assert!(
            sqrt_ratio_next_x32 < price_after_whole_input_amount,
            "price is less than price after whole input amount"
        );
        assert_eq!(
            sqrt_ratio_next_x32, 4316388712,
            "price is capped at price target"
        );

        // Δy = L (√P_upper - √P_lower), round up = ceil(2 * 10^8 (4316388712 - 4294967296) / 2^32)
        assert_eq!(amount_in, 997513);
        // Δx = L * (1 / √P_lower - 1 / √P_upper), floor = floor (2 * 10^8 * 2^32 (1/4294967296 - 1/4316388712))
        assert_eq!(amount_out, 992561);
        // amount_in * fee, ceil = ceil(997513 * 600/10^6)
        assert_eq!(fee_amount, 599);
        assert!(
            amount_in + fee_amount < amount as u64,
            "entire amount is not used"
        );
    }

    #[test]
    fn exact_amount_out_that_gets_capped_at_price_target_in_one_for_zero() {
        // exact out swap for token_1 -> token_0
        let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
        let sqrt_p_x32_target = encode_price_sqrt_x32(101, 100); // 4316388712

        let liquidity = 2 * u32::pow(10, 8) as u64;

        // amount of token_0 that must come out
        let amount = -i64::pow(10, 8); // negative for exact output swap
        let fee = 600;
        let zero_for_one = false;

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(sqrt_p_x32, sqrt_p_x32_target, liquidity, amount, fee);

        // √P' = √P * L / (L + Δx * √P), ceil
        //  = 4294967296 * 2 * 10^8 / (2 * 10^8 - 10^8 * 4294967296 / 2^32) = 8589934592
        // But we are capped at price_target = 4316388712
        let price_after_whole_output_amount = sqrt_price_math::get_next_sqrt_price_from_output(
            sqrt_p_x32,
            liquidity,
            -amount as u64,
            zero_for_one,
        );
        assert!(
            sqrt_ratio_next_x32 < price_after_whole_output_amount,
            "price is less than price after whole output amount"
        );
        assert_eq!(
            sqrt_ratio_next_x32, 4316388712,
            "price is capped at price target"
        );

        // Δy = L (√P_upper - √P_lower), round up = ceil(2 * 10^8 (4316388712 - 4294967296) / 2^32)
        assert_eq!(amount_in, 997513);
        // Δx = L * (1 / √P_lower - 1 / √P_upper), floor = floor (2 * 10^8 * 2^32 (1/4294967296 - 1/4316388712))
        assert_eq!(amount_out, 992561);
        assert!(
            amount_out < -amount as u64,
            "Entire amount out is not returned"
        ); // capped
           // amount_in * fee, ceil = ceil(997513 * 600/10^6)
        assert_eq!(fee_amount, 599);
    }

    /// Due to large price difference, the entire amount in is consumed without
    /// reaching the target
    ///
    #[test]
    fn exact_amount_in_that_is_fully_spent_in_one_for_zero() {
        let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
        let sqrt_p_x32_target = encode_price_sqrt_x32(1000, 100); // 13581879131
        let liquidity = 2 * u32::pow(10, 8) as u64;
        // amount of token_1 that must go in
        let amount = i64::pow(10, 8); // positive for exact input swap
        let fee = 600;
        let zero_for_one = false;

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(sqrt_p_x32, sqrt_p_x32_target, liquidity, amount, fee);

        assert_eq!(fee_amount, 60000); // amount * fee/10^6
        assert_eq!(amount_in, 99940000); // amount - fee_amount
        assert_eq!(
            amount_in + fee_amount,
            amount as u64,
            "entire amount is used"
        );

        // √P' = √P + Δy / L, floor = floor(4294967296 + 99940000 * 2^32 / (2 * 10^8)) = 6441162453
        let price_after_whole_input_amount_less_fee =
            sqrt_price_math::get_next_sqrt_price_from_input(
                sqrt_p_x32,
                liquidity,
                amount as u64 - fee_amount,
                zero_for_one,
            );
        assert!(
            price_after_whole_input_amount_less_fee < sqrt_p_x32_target,
            "price does not reach price target"
        );
        assert_eq!(
            sqrt_ratio_next_x32, price_after_whole_input_amount_less_fee,
            "price is equal to price after whole input amount"
        );

        // Δx = L * (1 / √P_lower - 1 / √P_upper), floor = floor (2 * 10^8 * 2^32 (1/4294967296 - 1/6441162453))
        assert_eq!(amount_out, 66639994);
    }

    /// Due to large price difference, entire amount out is delivered without
    /// reaching the target price
    ///
    #[test]
    fn exact_amount_out_that_is_fully_received_in_one_for_zero() {
        let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
        let sqrt_p_x32_target = encode_price_sqrt_x32(1000, 100); // 13581879131
        let liquidity = 2 * u32::pow(10, 8) as u64;
        // amount of token_0 that must go out
        let amount = -i64::pow(10, 8);
        let fee = 600;
        let zero_for_one = false;

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(sqrt_p_x32, sqrt_p_x32_target, liquidity, amount, fee);

        assert_eq!(amount_out, -amount as u64);

        // √P' = √P * L / (L + Δx * √P) = 4294967296 * 2 * 10^8 / (2 * 10^8 - 10^8 * 4294967296 / 2^32) = 8589934592
        let price_after_whole_output_amount = sqrt_price_math::get_next_sqrt_price_from_output(
            sqrt_p_x32,
            liquidity,
            -amount as u64,
            zero_for_one,
        );
        assert_eq!(
            sqrt_ratio_next_x32, price_after_whole_output_amount,
            "price is less than price after whole output amount"
        );
        assert_eq!(sqrt_ratio_next_x32, 8589934592);
        assert!(
            sqrt_ratio_next_x32 < sqrt_p_x32_target,
            "price does not reach price target"
        );

        // Δy = L (√P_upper - √P_lower), round up = ceil(2 * 10^8 (8589934592 - 4294967296) / 2^32)
        assert_eq!(amount_in, 200000000);
        assert_eq!(fee_amount, 120073); // ceil((600/10^6) / (1- 600/10^6) * 200000000)
    }

    /// In an exact output swap, the output amount cannot be crossed
    #[test]
    fn amount_out_is_capped_at_the_desired_amount_out() {
        let sqrt_p_x32 = encode_price_sqrt_x32(1, 1); // 4294967296
        let sqrt_p_x32_target = encode_price_sqrt_x32(100, 110); // 4095090639
        let liquidity = 2 * u32::pow(10, 8) as u64;
        let amount = -1; // token_1 out
        let fee = 1;

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(sqrt_p_x32, sqrt_p_x32_target, liquidity, amount, fee);

        assert_eq!(amount_out, 1);
        // √P' = √P + Δy / L, floor = floor(4294967296 - 1 * 2^32 / (2* 10^8))
        assert_eq!(sqrt_ratio_next_x32, 4294967274);
        // Δx = L * (1 / √P_lower - 1 / √P_upper), ceil = ceil(2 * 10^8 * 2^32 * ( 1/4294967274 - 1/ 4294967296))
        assert_eq!(amount_in, 2); // Prices are equal initially
        assert_eq!(fee_amount, 1); // ceil((600/10^6) / (1- 600/10^6) * 2)
    }

    /// When allowed price range is small and exact input amount is large,
    /// a large amount is converted into fees
    ///
    #[test]
    fn target_price_of_1_uses_partial_input_amount() {
        // exact input swap, token_0 -> token_1
        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(
            2,
            1,
            1,
            i64::pow(10, 8), // Δx
            1,
        );

        // √P' = √P * L / (L + Δx * √P), ceil = ceil(2 * 1 / (1 + 10^8 * 2 / 2^32))
        assert_eq!(sqrt_ratio_next_x32, 2);
        // Δy = L (√P_upper - √P_lower), round down for exact in = floor (L * (2- 2))
        assert_eq!(amount_out, 0);

        // In exact input swap, entire amount must go in
        // Since price difference is small, amount turns into fees instead of producing price impact
        // Δx = L * (1 / √P_lower - 1 / √P_upper), round up for exact in = ceil (L * (1/2 - 1/2)) = 0
        assert_eq!(amount_in, 0);
        assert_eq!(fee_amount, i64::pow(10, 8) as u64);
    }

    /// When amount remaining is very small relative to pool liquidity, price is not
    /// impacted due to integer division. Entire amount must then convert into fees.
    ///
    /// Cyclos uses u32 and U32.32 for liquidity and sqrt_price, whereas Uniswap
    /// uses u128 and U64.96 respectively. Therefore this condition only happens for
    /// amount_remaining = 1, i.e. 1 * 2^32 / u32::MAX = 0. Wheareas it is possible in
    /// Uniswap for a larger range of values.
    ///
    #[test]
    fn entire_input_amount_taken_as_fee() {
        // exact input swap, token_1 -> token_0
        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(
            100,
            100_000,
            u32::MAX as u64,
            1, // Δy
            1,
        );

        // √P' = √P' = √P + Δy / L, floor = floor(100 + 1 * 2^32 / (2^32 - 1)) = 100
        assert_eq!(sqrt_ratio_next_x32, 100); // no price impact
                                              // Δx and Δy are 0 since there is no price impact
        assert_eq!(amount_out, 0);
        assert_eq!(amount_in, 0);
        assert_eq!(fee_amount, 1); // entire input must convert into fees
    }

    /// Exact output amount can remain unmet if liquidity is low and
    /// available price range is small. Due to rounding down, output amount
    /// can become zero.
    ///
    #[test]
    fn handles_intermediate_insufficient_liquidity_in_zero_for_one_exact_output_case() {
        let sqrt_p_x32 = encode_price_sqrt_x32(4, 262144); // 16777216
        let sqrt_p_x32_target = sqrt_p_x32 * 9 / 10; // 15099494
        let liquidity = encode_liquidity(4, 262144); // 1024

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(
            sqrt_p_x32,
            sqrt_p_x32_target,
            liquidity,
            -4, // Δy out
            3000,
        );

        // Δy = L (√P_upper - √P_lower), round down = floor (1024 * (16777216 - 15099494) / 2^32) = 0
        assert_eq!(amount_out, 0);
        // Δx = L * (1 / √P_lower - 1 / √P_upper), ceil = ceil(1024 * 2^32 (1/15099494 - 1/16777216))
        assert_eq!(amount_in, 29128);
        // Price reaches target, trying to meet output amount
        assert_eq!(sqrt_ratio_next_x32, sqrt_p_x32_target);
        assert_eq!(fee_amount, 88); // ceil(0.003 / (1- 0.003) * 29178)
    }

    /// Exact output amount can remain unmet if liquidity is low and
    /// available price range is small. Output amount approaches virtual reserves
    /// from available liquidity.
    ///
    #[test]
    fn handles_intermediate_insufficient_liquidity_in_one_for_zero_exact_output_case() {
        let sqrt_p_x32 = encode_price_sqrt_x32(4, 262144); // 16777216
        let sqrt_p_x32_target = sqrt_p_x32 * 11 / 10; // 18454938
        let liquidity = encode_liquidity(4, 262144); // 1024

        let SwapStep {
            sqrt_ratio_next_x32,
            amount_in,
            amount_out,
            fee_amount,
        } = compute_swap_step(
            sqrt_p_x32,
            sqrt_p_x32_target,
            liquidity,
            -26214400, // Δx out
            3000,
        );

        // Δx = L * (1 / √P_lower - 1 / √P_upper), floor = ceil(1024 * 2^32 (1/16777216 - 1/18454938))
        assert_eq!(amount_out, 23831); // approaches reserves_0 = 262144

        // Δy = L (√P_upper - √P_lower), ceil = ceil (1024 * (18454938 - 16777216) / 2^32) = 1
        assert_eq!(amount_in, 1);
        // Price reaches target, trying to meet output amount
        assert_eq!(sqrt_ratio_next_x32, sqrt_p_x32_target);
        assert_eq!(fee_amount, 1); // ceil(0.003 / (1- 0.003) * 1)
    }
}
