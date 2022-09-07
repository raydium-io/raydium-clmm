use super::full_math::MulDiv;
use super::liquidity_amounts;
use super::sqrt_price_math;
use crate::states::config::FEE_RATE_DENOMINATOR_VALUE;

/// Result of a swap step
#[derive(Default, Debug)]
pub struct SwapStep {
    /// The price after swapping the amount in/out, not to exceed the price target
    pub sqrt_price_next_x64: u128,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee_amount: u64,
}

/// Computes the result of swapping some amount in, or amount out, given the parameters of the swap
pub fn compute_swap_step(
    sqrt_price_current_x64: u128,
    sqrt_price_target_x64: u128,
    liquidity: u128,
    amount_remaining: u64,
    fee_rate: u32,
    is_base_input: bool,
) -> SwapStep {
    let zero_for_one = sqrt_price_current_x64 >= sqrt_price_target_x64;
    // let exact_in = amount_remaining >= 0;
    let mut swap_step = SwapStep::default();
    if is_base_input {
        // round up amount_in
        // In exact input case, amount_remaining is positive
        let amount_remaining_less_fee = (amount_remaining as u64)
            .mul_div_floor(
                (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                FEE_RATE_DENOMINATOR_VALUE as u64,
            )
            .unwrap();
        swap_step.amount_in = if zero_for_one {
            liquidity_amounts::get_amount_0_delta_unsigned(
                sqrt_price_target_x64,
                sqrt_price_current_x64,
                liquidity,
                true,
            )
        } else {
            liquidity_amounts::get_amount_1_delta_unsigned(
                sqrt_price_current_x64,
                sqrt_price_target_x64,
                liquidity,
                true,
            )
        };
        swap_step.sqrt_price_next_x64 = if amount_remaining_less_fee >= swap_step.amount_in {
            sqrt_price_target_x64
        } else {
            sqrt_price_math::get_next_sqrt_price_from_input(
                sqrt_price_current_x64,
                liquidity,
                amount_remaining_less_fee,
                zero_for_one,
            )
        };
    } else {
        // round down amount_out
        swap_step.amount_out = if zero_for_one {
            liquidity_amounts::get_amount_1_delta_unsigned(
                sqrt_price_target_x64,
                sqrt_price_current_x64,
                liquidity,
                false,
            )
        } else {
            liquidity_amounts::get_amount_0_delta_unsigned(
                sqrt_price_current_x64,
                sqrt_price_target_x64,
                liquidity,
                false,
            )
        };
        // In exact output case, amount_remaining is negative
        swap_step.sqrt_price_next_x64 = if amount_remaining >= swap_step.amount_out {
            sqrt_price_target_x64
        } else {
            sqrt_price_math::get_next_sqrt_price_from_output(
                sqrt_price_current_x64,
                liquidity,
                amount_remaining ,
                zero_for_one,
            )
        }
    }

    // whether we reached the max possible price for the given ticks
    let max = sqrt_price_target_x64 == swap_step.sqrt_price_next_x64;
    // get the input / output amounts when target price is not reached
    if zero_for_one {
        // if max is reached for exact input case, entire amount_in is needed
        if !(max && is_base_input) {
            swap_step.amount_in = liquidity_amounts::get_amount_0_delta_unsigned(
                swap_step.sqrt_price_next_x64,
                sqrt_price_current_x64,
                liquidity,
                true,
            )
        };
        // if max is reached for exact output case, entire amount_out is needed
        if !(max && !is_base_input) {
            swap_step.amount_out = liquidity_amounts::get_amount_1_delta_unsigned(
                swap_step.sqrt_price_next_x64,
                sqrt_price_current_x64,
                liquidity,
                false,
            );
        };
    } else {
        if !(max && is_base_input) {
            swap_step.amount_in = liquidity_amounts::get_amount_1_delta_unsigned(
                sqrt_price_current_x64,
                swap_step.sqrt_price_next_x64,
                liquidity,
                true,
            )
        };
        if !(max && !is_base_input) {
            swap_step.amount_out = liquidity_amounts::get_amount_0_delta_unsigned(
                sqrt_price_current_x64,
                swap_step.sqrt_price_next_x64,
                liquidity,
                false,
            )
        };
    }

    // For exact output case, cap the output amount to not exceed the remaining output amount
    if !is_base_input && swap_step.amount_out > amount_remaining  {
        swap_step.amount_out = amount_remaining;
    }

    swap_step.fee_amount = if is_base_input && swap_step.sqrt_price_next_x64 != sqrt_price_target_x64 {
        // we didn't reach the target, so take the remainder of the maximum input as fee
        // swap dust is granted as fee
        (amount_remaining as u64)
            .checked_sub(swap_step.amount_in)
            .unwrap()
    } else {
        // take pip percentage as fee
        swap_step
            .amount_in
            .mul_div_ceil(
                fee_rate.into(),
                (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
            )
            .unwrap()
    };

    swap_step
}
