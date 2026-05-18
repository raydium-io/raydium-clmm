use crate::error::ErrorCode;
use crate::libraries::{
    big_num::U128, fixed_point_64, full_math::MulDiv, liquidity_math, swap_math, tick_math,
};
use crate::states::*;
use crate::util::*;
use anchor_lang::{prelude::*, solana_program};
use anchor_spl::token::{Token, TokenAccount};
use std::cell::RefMut;
use std::collections::VecDeque;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::Neg;

#[derive(Accounts)]
pub struct SwapSingle<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<Account<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<Account<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Box<Account<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Box<Account<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    #[account(mut, constraint = tick_array.load()?.pool_id == pool_state.key())]
    pub tick_array: AccountLoader<'info, TickArrayState>,
}

pub struct SwapAccounts<'b, 'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The user token account for input token
    pub input_token_account: Box<Account<'info, TokenAccount>>,

    /// The user token account for output token
    pub output_token_account: Box<Account<'info, TokenAccount>>,

    /// The vault token account for input token
    pub input_vault: Box<Account<'info, TokenAccount>>,

    /// The vault token account for output token
    pub output_vault: Box<Account<'info, TokenAccount>>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// The factory state to read protocol fees
    pub amm_config: &'b Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    pub pool_state: &'b mut AccountLoader<'info, PoolState>,

    /// The tick_array account of current or next initialized
    pub tick_array_state: &'b mut AccountLoader<'info, TickArrayState>,

    /// The program account for the oracle observation
    pub observation_state: &'b mut AccountLoader<'info, ObservationState>,
}

/// Aggregated outcome returned by `swap_internal` after the swap loop finishes.
#[derive(Debug, Clone, Copy)]
pub struct SwapInternalResult {
    /// Net token_0 amount the pool absorbs (input side) or releases (output side)
    pub amount_0: u64,
    /// Net token_1 amount the pool absorbs (input side) or releases (output side)
    pub amount_1: u64,
    /// Total AMM trade fee (lp + protocol + fund) charged in token_0 during the swap
    pub trade_fee_0: u64,
    /// Total AMM trade fee (lp + protocol + fund) charged in token_1 during the swap
    pub trade_fee_1: u64,
    /// Pool sqrt(price) (Q64.64) after the swap
    pub sqrt_price_x64: u128,
    /// Pool liquidity after the swap
    pub liquidity: u128,
    /// Pool current tick after the swap
    pub tick: i32,
}

// the top level state of the swap, the results of which are recorded in storage at the end
#[derive(Debug)]
pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    pub amount_specified_remaining: u64,
    // the amount already swapped out/in of the output/input asset
    pub amount_calculated: u64,
    // The latest sqrt price of the pool
    pub sqrt_price_x64: u128,
    // the tick associated with the current price
    pub tick: i32,
    // the global fee growth of the token that receives fees this swap (depends on fee_on and zero_for_one)
    pub fee_growth_global_x64: u128,
    // the amount of input token paid as lp fee
    pub lp_fee: u64,
    // the amount of input token paid as protocol fee
    pub protocol_fee: u64,
    // the amount of input token paid as fund fee
    pub fund_fee: u64,
    // the current liquidity in range
    pub liquidity: u128,

    // the sqrt price for the next tick
    pub sqrt_price_next_x64: u128,
    // the next tick to swap to from the current tick in the swap direction
    pub tick_next: i32,

    // The tick spacing of the pool, used to group ticks for dynamic fee calculation
    pub tick_spacing: u16,
    // The base fee rate (static component) of the pool
    pub base_fee_rate: u32,
    // The current tick spacing index, representing which tick group the current price belongs to.
    // This is used to track volatility for dynamic fee calculation.
    pub tick_spacing_index: i32,
    // Dynamic fee configuration and state, including volatility accumulator and reference data.
    // None if dynamic fee is not enabled for this pool.
    pub dynamic_fee_info: Option<DynamicFeeInfo>,
}

impl SwapState {
    pub fn new(
        pool_state: &PoolState,
        amount_specified: u64,
        base_fee_rate: u32,
        zero_for_one: bool,
        block_timestamp: u64,
    ) -> Result<Self> {
        let mut state = Self {
            amount_specified_remaining: amount_specified,
            amount_calculated: 0,
            sqrt_price_x64: pool_state.sqrt_price_x64,
            tick: pool_state.tick_current,
            fee_growth_global_x64: if pool_state.is_fee_on_token0(zero_for_one) {
                pool_state.fee_growth_global_0_x64
            } else {
                pool_state.fee_growth_global_1_x64
            },
            lp_fee: 0,
            protocol_fee: 0,
            fund_fee: 0,
            liquidity: pool_state.liquidity,
            sqrt_price_next_x64: 0,
            tick_next: 0,
            base_fee_rate: base_fee_rate,
            tick_spacing: pool_state.tick_spacing,
            tick_spacing_index: 0,
            dynamic_fee_info: pool_state.get_dynamic_fee_info(),
        };
        if let Some(dynamic_fee_info) = &mut state.dynamic_fee_info {
            state.tick_spacing_index = tick_spacing_index_from_tick(state.tick, state.tick_spacing);
            dynamic_fee_info.update_reference(state.tick_spacing_index, block_timestamp)?;
        }
        Ok(state)
    }

    /// Apply swap step result by updating remaining and calculated amounts
    pub fn apply_swap_amounts(
        &mut self,
        amount_in: u64,
        amount_out: u64,
        fee_amount: u64,
        is_base_input: bool,
        is_fee_on_input: bool,
        protocol_fee_rate: u32,
        fund_fee_rate: u32,
    ) -> Result<()> {
        // Calculate the actual amount_in consumed by user
        // If fee is from input, user pays amount_in + fee_amount; otherwise just amount_in
        let amount_in_consumed = if is_fee_on_input {
            amount_in
                .checked_add(fee_amount)
                .ok_or(ErrorCode::CalculateOverflow)?
        } else {
            amount_in
        };

        if is_base_input {
            // Exact Input Swap: deduct consumed input from remaining, add output to calculated
            self.amount_specified_remaining = self
                .amount_specified_remaining
                .checked_sub(amount_in_consumed)
                .ok_or(ErrorCode::CalculateOverflow)?;
            // amount_out is already net output (fee already deducted if fee is from output)
            self.amount_calculated = self
                .amount_calculated
                .checked_add(amount_out)
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else {
            // Exact Output Swap: deduct output from remaining, add consumed input to calculated
            self.amount_specified_remaining = self
                .amount_specified_remaining
                .checked_sub(amount_out)
                .ok_or(ErrorCode::CalculateOverflow)?;
            self.amount_calculated = self
                .amount_calculated
                .checked_add(amount_in_consumed)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }
        self.spilt_fees(fee_amount, protocol_fee_rate, fund_fee_rate)?;
        Ok(())
    }

    pub fn spilt_fees(
        &mut self,
        fee_amont: u64,
        protocol_fee_rate: u32,
        fund_fee_rate: u32,
    ) -> Result<()> {
        let mut remaining_fee = fee_amont;
        // Process protocol fee
        if protocol_fee_rate > 0 {
            let protocol_fee_delta = U128::from(fee_amont)
                .checked_mul(protocol_fee_rate.into())
                .and_then(|v| v.checked_div(FEE_RATE_DENOMINATOR_VALUE.into()))
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64();
            self.protocol_fee = self
                .protocol_fee
                .checked_add(protocol_fee_delta)
                .ok_or(ErrorCode::CalculateOverflow)?;
            remaining_fee = remaining_fee
                .checked_sub(protocol_fee_delta)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }

        // Process fund fee
        if fund_fee_rate > 0 {
            let fund_fee_delta = U128::from(fee_amont)
                .checked_mul(fund_fee_rate.into())
                .and_then(|v| v.checked_div(FEE_RATE_DENOMINATOR_VALUE.into()))
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64();

            self.fund_fee = self
                .fund_fee
                .checked_add(fund_fee_delta)
                .ok_or(ErrorCode::CalculateOverflow)?;
            remaining_fee = remaining_fee
                .checked_sub(fund_fee_delta)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }

        // Update global fee tracker
        if self.liquidity > 0 {
            let fee_growth_global_x64_delta = U128::from(remaining_fee)
                .mul_div_floor(U128::from(fixed_point_64::Q64), U128::from(self.liquidity))
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u128();

            self.fee_growth_global_x64 = self
                .fee_growth_global_x64
                .wrapping_add(fee_growth_global_x64_delta);
            self.lp_fee = self
                .lp_fee
                .checked_add(remaining_fee)
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else {
            self.protocol_fee = self
                .protocol_fee
                .checked_add(remaining_fee)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }
        Ok(())
    }

    /// Settle the per-token deltas and trade-fee split after the swap loop.
    /// Returns `(amount_0, amount_1, trade_fee_0, trade_fee_1)`:
    /// - `amount_*` are the net token_0 / token_1 deltas the pool absorbs or releases.
    /// - `trade_fee_*` is the total AMM trade fee (lp + protocol + fund) charged on the
    ///   fee-side token (the other is zero).
    pub fn settle_amounts(
        &self,
        amount_specified: u64,
        zero_for_one: bool,
        is_base_input: bool,
        fee_on_token0: bool,
    ) -> Result<(u64, u64, u64, u64)> {
        let consumed = amount_specified
            .checked_sub(self.amount_specified_remaining)
            .ok_or(ErrorCode::CalculateOverflow)?;
        let (amount_0, amount_1) = if zero_for_one == is_base_input {
            (consumed, self.amount_calculated)
        } else {
            (self.amount_calculated, consumed)
        };

        let total_trade_fee = self
            .lp_fee
            .checked_add(self.protocol_fee)
            .and_then(|v| v.checked_add(self.fund_fee))
            .ok_or(ErrorCode::CalculateOverflow)?;
        let (trade_fee_0, trade_fee_1) = if fee_on_token0 {
            (total_trade_fee, 0u64)
        } else {
            (0u64, total_trade_fee)
        };

        Ok((amount_0, amount_1, trade_fee_0, trade_fee_1))
    }

    fn get_target_price_based_on_next_tick(
        &mut self,
        tick_next: i32,
        zero_for_one: bool,
        sqrt_price_limit_x64: u128,
    ) -> Result<u128> {
        // Clamp tick_next to valid range
        self.tick_next = tick_next;
        if self.tick_next < tick_math::MIN_TICK {
            self.tick_next = tick_math::MIN_TICK;
        } else if self.tick_next > tick_math::MAX_TICK {
            self.tick_next = tick_math::MAX_TICK;
        }

        // Calculate sqrt_price for the next tick
        self.sqrt_price_next_x64 = tick_math::get_sqrt_price_at_tick(self.tick_next)?;

        // Determine target price: either the next tick price or the limit price
        let target_price = if (zero_for_one && self.sqrt_price_next_x64 < sqrt_price_limit_x64)
            || (!zero_for_one && self.sqrt_price_next_x64 > sqrt_price_limit_x64)
        {
            sqrt_price_limit_x64
        } else {
            self.sqrt_price_next_x64
        };

        // Validate swap direction
        if zero_for_one {
            require_gte!(self.tick, self.tick_next);
            require_gte!(self.sqrt_price_x64, self.sqrt_price_next_x64);
            require_gte!(self.sqrt_price_x64, target_price);
        } else {
            require_gt!(self.tick_next, self.tick);
            require_gte!(self.sqrt_price_next_x64, self.sqrt_price_x64);
            require_gte!(target_price, self.sqrt_price_x64);
        }

        Ok(target_price)
    }

    pub fn update_volatility_accumulator(&mut self) -> Result<()> {
        if let Some(dynamic_fee_info) = &mut self.dynamic_fee_info {
            dynamic_fee_info.update_volatility_accumulator(self.tick_spacing_index)?;
        }
        Ok(())
    }

    pub fn update_dynamic_fee_index(
        &mut self,
        zero_for_one: bool,
        is_skipped_tick_spacing: bool,
    ) -> Result<()> {
        if let Some(dynamic_fee_info) = &self.dynamic_fee_info {
            if is_skipped_tick_spacing {
                let tick_index = if self.sqrt_price_x64 == self.sqrt_price_next_x64 {
                    self.tick_next
                } else {
                    self.tick
                };
                let mut tick_spacing_index =
                    tick_spacing_index_from_tick(tick_index, self.tick_spacing);
                if !zero_for_one && tick_index % (self.tick_spacing as i32) == 0 {
                    tick_spacing_index = tick_spacing_index - 1;
                }
                self.tick_spacing_index = tick_spacing_index;

                if dynamic_fee_info.volatility_accumulator
                    != dynamic_fee_info.max_volatility_accumulator
                {
                    self.update_volatility_accumulator()?;
                }
            }
            self.tick_spacing_index += if zero_for_one { -1 } else { 1 };
        }
        Ok(())
    }

    pub fn update_volatility_accumulator_on_price(&mut self) -> Result<()> {
        if self.dynamic_fee_info.is_some() {
            let tick_index = tick_math::get_tick_at_sqrt_price(self.sqrt_price_x64)?;
            let final_tick_spacing_index =
                tick_spacing_index_from_tick(tick_index, self.tick_spacing);
            if self.tick_spacing_index != final_tick_spacing_index {
                self.tick_spacing_index = final_tick_spacing_index;
                self.update_volatility_accumulator()?;
            }
        }
        Ok(())
    }

    pub fn get_spacing_bounded_price(
        &self,
        target_price: u128,
        zero_for_one: bool,
    ) -> Result<(bool, u128)> {
        if let Some(dynamic_fee_info) = &self.dynamic_fee_info {
            if self.liquidity == 0
                || dynamic_fee_info.volatility_accumulator
                    == dynamic_fee_info.max_volatility_accumulator
            {
                return Ok((true, target_price));
            }

            let tick_spacing_i32 = i32::from(self.tick_spacing);
            let bounded_tick = if zero_for_one {
                self.tick_spacing_index.saturating_mul(tick_spacing_i32)
            } else {
                self.tick_spacing_index
                    .saturating_add(1)
                    .saturating_mul(tick_spacing_i32)
            };
            #[cfg(feature = "enable-log")]
            msg!(
                "state.tick:{}, state.tick_spacing_index:{}, bounded_tick:{}",
                self.tick,
                self.tick_spacing_index,
                bounded_tick
            );
            let bounded_sqrt_price = tick_math::get_sqrt_price_at_tick(
                bounded_tick.clamp(tick_math::MIN_TICK, tick_math::MAX_TICK),
            )?;

            if zero_for_one {
                Ok((false, target_price.max(bounded_sqrt_price)))
            } else {
                Ok((false, target_price.min(bounded_sqrt_price)))
            }
        } else {
            Ok((true, target_price))
        }
    }

    pub fn get_total_fee_rate(&self) -> Result<u32> {
        // Use base + dynamic fee if dynamic fee is enabled
        if let Some(dynamic_fee_info) = &self.dynamic_fee_info {
            let dynamic_fee_rate =
                Self::compute_dynamic_fee_rate(dynamic_fee_info, self.tick_spacing)?;
            let total_fee_rate = self.base_fee_rate + dynamic_fee_rate;
            return Ok(total_fee_rate.min(MAX_FEE_RATE_NUMERATOR));
        }
        // Use base fee if not in launch phase and dynamic fee is disabled
        Ok(self.base_fee_rate)
    }

    /// Computes the dynamic fee rate based on volatility accumulator.
    ///
    /// The dynamic fee rate is calculated using a quadratic formula that maps the squared
    /// volatility accumulator to a fee rate. This creates a non-linear relationship where
    /// higher volatility results in exponentially higher fees, providing stronger protection
    /// against market manipulation during volatile periods.
    fn compute_dynamic_fee_rate(
        dynamic_fee_info: &DynamicFeeInfo,
        tick_spacing: u16,
    ) -> Result<u32> {
        let crossed = dynamic_fee_info.volatility_accumulator * tick_spacing as u32;

        // Square the crossed value to create quadratic fee scaling
        let squared = u64::from(crossed) * u64::from(crossed);

        let denominator = U128::from(DYNAMIC_FEE_CONTROL_DENOMINATOR)
            * U128::from(VOLATILITY_ACCUMULATOR_SCALE)
            * U128::from(VOLATILITY_ACCUMULATOR_SCALE);

        // Compute fee rate using ceiling division to ensure minimum fee protection
        let fee_rate = U128::from(dynamic_fee_info.dynamic_fee_control)
            .mul_div_ceil(U128::from(squared), denominator)
            .ok_or(ErrorCode::CalculateOverflow)?
            .as_u128();
        // bound the fee rate to the maximum fee rate
        if fee_rate > MAX_FEE_RATE_NUMERATOR as u128 {
            Ok(MAX_FEE_RATE_NUMERATOR)
        } else {
            Ok(fee_rate as u32)
        }
    }
}

pub fn swap_internal<'b, 'c: 'info, 'info>(
    amm_config: &AmmConfig,
    pool_state: &mut RefMut<PoolState>,
    tick_array_states: &mut VecDeque<RefMut<TickArrayState>>,
    observation_state: &mut RefMut<ObservationState>,
    tickarray_bitmap_extension_info: Option<&'c AccountInfo<'info>>,
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    zero_for_one: bool,
    is_base_input: bool,
    block_timestamp: u32,
) -> Result<SwapInternalResult> {
    require!(amount_specified != 0, ErrorCode::ZeroAmountSpecified);
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::Swap) {
        return err!(ErrorCode::NotApproved);
    }
    require!(
        if zero_for_one {
            sqrt_price_limit_x64 < pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 > tick_math::MIN_SQRT_PRICE_X64
        } else {
            sqrt_price_limit_x64 > pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 < tick_math::MAX_SQRT_PRICE_X64
        },
        ErrorCode::SqrtPriceLimitOverflow
    );

    let updated_reward_infos = pool_state.update_reward_infos(block_timestamp as u64)?;
    // check observation account is owned by the pool
    require_keys_eq!(observation_state.pool_id, pool_state.key());

    let (mut first_tick_array_contains_pool_tick, first_valid_tick_array_start_index) = pool_state
        .first_tick_array_index_with_extension_info(
            tickarray_bitmap_extension_info,
            zero_for_one,
        )?;
    let mut current_valid_tick_array_start_index = first_valid_tick_array_start_index;

    let mut tick_array_current = tick_array_states
        .pop_front()
        .ok_or(ErrorCode::NotEnoughTickArrayAccount)?;
    // find the first active tick array account
    for _ in 0..tick_array_states.len() {
        if tick_array_current.start_tick_index == current_valid_tick_array_start_index {
            break;
        }
        tick_array_current = tick_array_states
            .pop_front()
            .ok_or(ErrorCode::NotEnoughTickArrayAccount)?;
    }
    // check the first tick_array account is owned by the pool
    require_keys_eq!(tick_array_current.pool_id, pool_state.key());
    // check first tick array account is correct
    require_eq!(
        tick_array_current.start_tick_index,
        current_valid_tick_array_start_index,
        ErrorCode::InvalidFirstTickArrayAccount
    );

    // Determine if fee should be collected from input token (only need to calculate once)
    let is_fee_on_input = pool_state.is_fee_on_input(zero_for_one);
    let mut state = SwapState::new(
        pool_state,
        amount_specified,
        amm_config.trade_fee_rate,
        zero_for_one,
        block_timestamp as u64,
    )?;

    // Main swap loop: continue swapping until we've consumed all input/output or reached the price limit
    // Each iteration processes one step from current price to the next initialized tick
    while state.amount_specified_remaining != 0 && state.sqrt_price_x64 != sqrt_price_limit_x64 {
        #[cfg(feature = "enable-log")]
        msg!("begin, is_base_input:{}, state.liquidity:{}, state.tick:{}, state.sqrt_price_x64:{}, state.tick_spacing_index:{}", is_base_input, state.liquidity, state.tick, state.sqrt_price_x64, state.tick_spacing_index);

        let mut next_initialized_tick = {
            // First, try to find next initialized tick in current tick array
            if let Some(tick_state) = tick_array_current.next_initialized_tick(
                state.tick,
                pool_state.tick_spacing,
                zero_for_one,
            )? {
                *tick_state
            }
            // If not found and the first tick array doesn't contain pool's current tick,
            // use the first initialized tick in current array (only happens once in the first iteration)
            else if !first_tick_array_contains_pool_tick {
                first_tick_array_contains_pool_tick = true;
                *tick_array_current.first_initialized_tick(zero_for_one)?
            }
            // Otherwise, need to move to next tick array
            else {
                let next_tick_array_index = pool_state
                    .next_tick_array_index_with_extension_info(
                        tickarray_bitmap_extension_info,
                        current_valid_tick_array_start_index,
                        zero_for_one,
                    )?
                    .ok_or(ErrorCode::LiquidityInsufficient)?;

                // Advance to the next tick array
                while tick_array_current.start_tick_index != next_tick_array_index {
                    tick_array_current = tick_array_states
                        .pop_front()
                        .ok_or(ErrorCode::NotEnoughTickArrayAccount)?;
                    // check the tick_array account is owned by the pool
                    require_keys_eq!(tick_array_current.pool_id, pool_state.key());
                }
                current_valid_tick_array_start_index = next_tick_array_index;

                *tick_array_current.first_initialized_tick(zero_for_one)?
            }
        };
        #[cfg(feature = "enable-log")]
        msg!(
            "next_initialized_tick:{}, tick_array_current:{}",
            identity(next_initialized_tick.tick),
            tick_array_current.key().to_string(),
        );
        require_eq!(next_initialized_tick.is_initialized(), true);

        let target_price = state.get_target_price_based_on_next_tick(
            next_initialized_tick.tick,
            zero_for_one,
            sqrt_price_limit_x64,
        )?;
        #[cfg(feature = "enable-log")]
        msg!(
            "state.tick_next:{}, state.sqrt_price_next_x64:{}",
            state.tick_next,
            state.sqrt_price_next_x64
        );

        let mut liquidity_next = state.liquidity;
        loop {
            state.update_volatility_accumulator()?;
            let total_fee_rate = state.get_total_fee_rate()?;
            let (is_skipped_tick_spacing, bounded_price) =
                state.get_spacing_bounded_price(target_price, zero_for_one)?;

            let is_price_change = state.sqrt_price_x64 != bounded_price;
            let swap_computed_result = if is_price_change {
                let swap_computed_result = swap_math::compute_swap(
                    state.sqrt_price_x64,
                    bounded_price,
                    state.liquidity,
                    state.amount_specified_remaining,
                    total_fee_rate,
                    is_base_input,
                    zero_for_one,
                    is_fee_on_input,
                )?;
                #[cfg(feature = "enable-log")]
                msg!(
                    "swap_computed_result: amount_in:{}, amount_out:{}, fee_amount:{}",
                    swap_computed_result.amount_in,
                    swap_computed_result.amount_out,
                    swap_computed_result.fee_amount
                );
                state.apply_swap_amounts(
                    swap_computed_result.amount_in,
                    swap_computed_result.amount_out,
                    swap_computed_result.fee_amount,
                    is_base_input,
                    is_fee_on_input,
                    amm_config.protocol_fee_rate,
                    amm_config.fund_fee_rate,
                )?;
                swap_computed_result
            } else {
                swap_math::SwapComputationResult::new(bounded_price)
            };
            let limit_order_unfilled_amount_before =
                next_initialized_tick.limit_order_unfilled_amount()?;
            if state.sqrt_price_next_x64 == swap_computed_result.sqrt_price_next_x64 {
                // try to match limit orders on this tick
                let limit_order_result = next_initialized_tick.match_limit_order(
                    state.amount_specified_remaining,
                    zero_for_one,
                    is_base_input,
                    total_fee_rate,
                    is_fee_on_input,
                )?;

                if limit_order_result.amount_in > 0 {
                    #[cfg(feature = "enable-log")]
                    msg!(
                        "limit_order_result: amount_in:{}, amount_out:{}, amm_fee_amount:{}",
                        limit_order_result.amount_in,
                        limit_order_result.amount_out,
                        limit_order_result.amm_fee_amount
                    );
                    state.apply_swap_amounts(
                        limit_order_result.amount_in,
                        limit_order_result.amount_out,
                        limit_order_result.amm_fee_amount,
                        is_base_input,
                        is_fee_on_input,
                        amm_config.protocol_fee_rate,
                        amm_config.fund_fee_rate,
                    )?;
                }

                if !next_initialized_tick.is_initialized() {
                    tick_array_current.update_initialized_tick_count(false)?;
                    if tick_array_current.initialized_tick_count == 0 {
                        pool_state.flip_tick_array_bit(
                            tickarray_bitmap_extension_info,
                            tick_array_current.start_tick_index,
                        )?;
                    }
                }

                if next_initialized_tick.has_liquidity()
                    && !next_initialized_tick.has_limit_orders()
                {
                    // Use current fee growth for each token: the one that receives fees this swap
                    // is updated in state, the other stays at pool value.
                    let fee_on_token0 = pool_state.is_fee_on_token0(zero_for_one);
                    let mut liquidity_net = next_initialized_tick.cross(
                        if fee_on_token0 {
                            state.fee_growth_global_x64
                        } else {
                            pool_state.fee_growth_global_0_x64
                        },
                        if fee_on_token0 {
                            pool_state.fee_growth_global_1_x64
                        } else {
                            state.fee_growth_global_x64
                        },
                        &updated_reward_infos,
                    );
                    if zero_for_one {
                        liquidity_net = liquidity_net.neg();
                    }
                    liquidity_next = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
                }

                tick_array_current.update_tick_state(
                    next_initialized_tick.tick,
                    pool_state.tick_spacing.into(),
                    next_initialized_tick,
                )?;

                // Update tick based on limit order status and swap direction
                // The tick assignment rule:
                // - zero_for_one=true && has_limit_orders: tick = tick_next
                // - zero_for_one=false && has_limit_orders: tick = tick_next - 1
                // - zero_for_one=true && !has_limit_orders: tick = tick_next - 1
                // - zero_for_one=false && !has_limit_orders: tick = tick_next
                state.tick = if (zero_for_one && !next_initialized_tick.has_limit_orders())
                    || (!zero_for_one && next_initialized_tick.has_limit_orders())
                {
                    state.tick_next - 1
                } else {
                    state.tick_next
                };
            } else if state.sqrt_price_x64 != swap_computed_result.sqrt_price_next_x64 {
                // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
                // if only a small amount of quantity is traded, the input may be consumed by fees, resulting in no price change. If state.sqrt_price_x64, i.e., the latest price in the pool, is used to recalculate the tick, some errors may occur.
                // for example, if zero_for_one, and the price falls exactly on an initialized tick t after the first trade, then at this point, pool.sqrtPriceX64 = get_sqrt_price_at_tick(t), while pool.tick = t-1. if the input quantity of the
                // second trade is very small and the pool price does not change after the transaction, if the tick is recalculated, pool.tick will be equal to t, which is incorrect.
                state.tick =
                    tick_math::get_tick_at_sqrt_price(swap_computed_result.sqrt_price_next_x64)?;
            }
            state.sqrt_price_x64 = swap_computed_result.sqrt_price_next_x64;
            if state.amount_specified_remaining == 0 || state.sqrt_price_x64 == target_price {
                let limit_order_unfilled_amount_after =
                    next_initialized_tick.limit_order_unfilled_amount()?;
                // One of the two parties must be equal to 0 to exit the loop
                // If a limit order has been executed and the active swap amount is not zero, then the remaining amount of the limit order must be zero, otherwise it's an abnormal situation
                if state.amount_specified_remaining != 0
                    && limit_order_unfilled_amount_after != limit_order_unfilled_amount_before
                {
                    require_eq!(limit_order_unfilled_amount_after, 0);
                }
                break;
            }
            state.update_dynamic_fee_index(zero_for_one, is_skipped_tick_spacing)?;
        }
        state.liquidity = liquidity_next;
    }
    // At the end of the entire swap loop, `updating_dynamic_fee_index` does not always guarantee that the tick_spacing_index lands in the correct position.
    // Therefore, we recalculate its position here based on the current price and update the volatility accumulator.
    state.update_volatility_accumulator_on_price()?;

    #[cfg(feature = "enable-log")]
    msg!("end, state:{:#?}", state);

    // Update pool state with final swap results
    if state.tick != pool_state.tick_current {
        // Update observation with previous tick before updating current tick
        observation_state.update(block_timestamp, pool_state.tick_current);
    }

    let (amount_0, amount_1, trade_fee_0, trade_fee_1) = state.settle_amounts(
        amount_specified,
        zero_for_one,
        is_base_input,
        pool_state.is_fee_on_token0(zero_for_one),
    )?;

    pool_state.update_after_swap(
        state.tick,
        state.sqrt_price_x64,
        state.liquidity,
        state.lp_fee,
        state.protocol_fee,
        state.fund_fee,
        state.fee_growth_global_x64,
        zero_for_one,
        state.dynamic_fee_info,
    )?;
    Ok(SwapInternalResult {
        amount_0,
        amount_1,
        trade_fee_0,
        trade_fee_1,
        sqrt_price_x64: pool_state.sqrt_price_x64,
        liquidity: pool_state.liquidity,
        tick: pool_state.tick_current,
    })
}

/// Performs a single exact input/output swap
/// if is_base_input = true, return value is the max_amount_out, otherwise is min_amount_in
pub fn exact_internal<'b, 'c: 'info, 'info>(
    ctx: &mut SwapAccounts<'b, 'info>,
    remaining_accounts: &'c [AccountInfo<'info>],
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<u64> {
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;

    let swap_result: SwapInternalResult;
    let zero_for_one;
    let swap_price_before;

    let input_balance_before = ctx.input_vault.amount;
    let output_balance_before = ctx.output_vault.amount;

    {
        swap_price_before = ctx.pool_state.load()?.sqrt_price_x64;
        let pool_state = &mut ctx.pool_state.load_mut()?;
        zero_for_one = ctx.input_vault.mint == pool_state.token_mint_0;

        require_gt!(block_timestamp, pool_state.open_time);

        require!(
            if zero_for_one {
                ctx.input_vault.key() == pool_state.token_vault_0
                    && ctx.output_vault.key() == pool_state.token_vault_1
            } else {
                ctx.input_vault.key() == pool_state.token_vault_1
                    && ctx.output_vault.key() == pool_state.token_vault_0
            },
            ErrorCode::InvalidInputPoolVault
        );

        let mut tickarray_bitmap_extension = None;
        let tick_array_states = &mut VecDeque::new();
        tick_array_states.push_back(ctx.tick_array_state.load_mut()?);

        let tick_array_bitmap_extension_key = TickArrayBitmapExtension::key(pool_state.key());
        for account_info in remaining_accounts.into_iter() {
            if account_info.key().eq(&tick_array_bitmap_extension_key) {
                tickarray_bitmap_extension = Some(account_info);
                continue;
            }
            tick_array_states.push_back(AccountLoad::load_data_mut(account_info)?);
        }

        swap_result = swap_internal(
            &ctx.amm_config,
            pool_state,
            tick_array_states,
            &mut ctx.observation_state.load_mut()?,
            tickarray_bitmap_extension,
            amount_specified,
            if sqrt_price_limit_x64 == 0 {
                if zero_for_one {
                    tick_math::MIN_SQRT_PRICE_X64 + 1
                } else {
                    tick_math::MAX_SQRT_PRICE_X64 - 1
                }
            } else {
                sqrt_price_limit_x64
            },
            zero_for_one,
            is_base_input,
            oracle::block_timestamp(),
        )?;

        #[cfg(feature = "enable-log")]
        msg!(
            "exact_swap_internal, is_base_input:{}, amount_0: {}, amount_1: {}",
            is_base_input,
            swap_result.amount_0,
            swap_result.amount_1
        );
        require!(
            swap_result.amount_0 != 0 && swap_result.amount_1 != 0,
            ErrorCode::TooSmallInputOrOutputAmount
        );
    }
    let (token_account_0, token_account_1, vault_0, vault_1) = if zero_for_one {
        (
            ctx.input_token_account.clone(),
            ctx.output_token_account.clone(),
            ctx.input_vault.clone(),
            ctx.output_vault.clone(),
        )
    } else {
        (
            ctx.output_token_account.clone(),
            ctx.input_token_account.clone(),
            ctx.output_vault.clone(),
            ctx.input_vault.clone(),
        )
    };

    emit!(SwapEvent {
        pool_state: ctx.pool_state.key(),
        sender: ctx.signer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0: swap_result.amount_0,
        transfer_fee_0: 0,
        amount_1: swap_result.amount_1,
        transfer_fee_1: 0,
        zero_for_one,
        sqrt_price_x64: swap_result.sqrt_price_x64,
        liquidity: swap_result.liquidity,
        tick: swap_result.tick,
        trade_fee_0: swap_result.trade_fee_0,
        trade_fee_1: swap_result.trade_fee_1,
    });

    if zero_for_one {
        //  x -> y, deposit x token from user to pool vault.
        transfer_from_user_to_pool_vault(
            &ctx.signer,
            &token_account_0.to_account_info(),
            &vault_0.to_account_info(),
            None,
            &ctx.token_program,
            None,
            swap_result.amount_0,
        )?;
        // x -> y，transfer y token from pool vault to user.
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_1.to_account_info(),
            &token_account_1.to_account_info(),
            None,
            &ctx.token_program,
            None,
            swap_result.amount_1,
        )?;
    } else {
        transfer_from_user_to_pool_vault(
            &ctx.signer,
            &token_account_1.to_account_info(),
            &vault_1.to_account_info(),
            None,
            &ctx.token_program,
            None,
            swap_result.amount_1,
        )?;
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_0.to_account_info(),
            &token_account_0.to_account_info(),
            None,
            &ctx.token_program,
            None,
            swap_result.amount_0,
        )?;
    }
    ctx.output_vault.reload()?;
    ctx.input_vault.reload()?;

    if zero_for_one {
        require_gte!(swap_price_before, swap_result.sqrt_price_x64);
    } else {
        require_gte!(swap_result.sqrt_price_x64, swap_price_before);
    }
    if sqrt_price_limit_x64 == 0 {
        // Does't allow partial filled without specified limit_price.
        if is_base_input {
            if zero_for_one {
                require_eq!(amount_specified, swap_result.amount_0);
            } else {
                require_eq!(amount_specified, swap_result.amount_1);
            }
        } else {
            if zero_for_one {
                require_eq!(amount_specified, swap_result.amount_1);
            } else {
                require_eq!(amount_specified, swap_result.amount_0);
            }
        }
    }

    if is_base_input {
        output_balance_before
            .checked_sub(ctx.output_vault.amount)
            .ok_or(ErrorCode::CalculateOverflow.into())
    } else {
        ctx.input_vault
            .amount
            .checked_sub(input_balance_before)
            .ok_or(ErrorCode::CalculateOverflow.into())
    }
}

pub fn swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let amount = exact_internal(
        &mut SwapAccounts {
            signer: ctx.accounts.payer.clone(),
            amm_config: &ctx.accounts.amm_config,
            input_token_account: ctx.accounts.input_token_account.clone(),
            output_token_account: ctx.accounts.output_token_account.clone(),
            input_vault: ctx.accounts.input_vault.clone(),
            output_vault: ctx.accounts.output_vault.clone(),
            token_program: ctx.accounts.token_program.clone(),
            pool_state: &mut ctx.accounts.pool_state,
            tick_array_state: &mut ctx.accounts.tick_array,
            observation_state: &mut ctx.accounts.observation_state,
        },
        ctx.remaining_accounts,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
    )?;
    if is_base_input {
        require!(
            amount >= other_amount_threshold,
            ErrorCode::TooLittleOutputReceived
        );
    } else {
        require!(
            amount <= other_amount_threshold,
            ErrorCode::TooMuchInputPaid
        );
    }

    Ok(())
}

#[cfg(test)]
mod swap_test {
    use super::*;
    use crate::states::pool_test::build_pool;
    use crate::states::tick_array_test::{
        build_tick, build_tick_array_with_tick_states, TickArrayInfo,
    };
    use liquidity_math::get_delta_amounts_signed;
    use rand::Rng;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::vec;
    use tick_array_bitmap_extension_test::build_tick_array_bitmap_extension_info;

    pub fn get_tick_array_states_mut(
        deque_tick_array_states: &VecDeque<RefCell<TickArrayState>>,
    ) -> RefCell<VecDeque<RefMut<TickArrayState>>> {
        let mut tick_array_states = VecDeque::new();

        for tick_array_state in deque_tick_array_states {
            tick_array_states.push_back(tick_array_state.borrow_mut());
        }
        RefCell::new(tick_array_states)
    }

    fn build_swap_param<'info>(
        tick_current: i32,
        tick_spacing: u16,
        sqrt_price_x64: u128,
        liquidity: u128,
        tick_array_infos: Vec<TickArrayInfo>,
    ) -> (
        AmmConfig,
        RefCell<PoolState>,
        VecDeque<RefCell<TickArrayState>>,
        RefCell<ObservationState>,
    ) {
        let amm_config = AmmConfig {
            trade_fee_rate: 1000,
            tick_spacing,
            ..Default::default()
        };
        let pool_state = build_pool(tick_current, tick_spacing, sqrt_price_x64, liquidity);

        let observation_state = RefCell::new(ObservationState::default());
        observation_state.borrow_mut().pool_id = pool_state.borrow().key();

        let mut tick_array_states: VecDeque<RefCell<TickArrayState>> = VecDeque::new();
        for tick_array_info in tick_array_infos {
            tick_array_states.push_back(build_tick_array_with_tick_states(
                pool_state.borrow().key(),
                tick_array_info.start_tick_index,
                tick_spacing,
                tick_array_info.ticks,
            ));
            pool_state
                .borrow_mut()
                .flip_tick_array_bit(None, tick_array_info.start_tick_index)
                .unwrap();
        }

        (amm_config, pool_state, tick_array_states, observation_state)
    }

    pub struct OpenPositionParam {
        pub amount_0: u64,
        pub amount_1: u64,
        // pub liquidity: u128,
        pub tick_lower: i32,
        pub tick_upper: i32,
    }

    fn setup_swap_test<'info>(
        start_tick: i32,
        tick_spacing: u16,
        position_params: Vec<OpenPositionParam>,
        zero_for_one: bool,
    ) -> (
        AmmConfig,
        RefCell<PoolState>,
        VecDeque<RefCell<TickArrayState>>,
        RefCell<ObservationState>,
        &'static AccountInfo<'static>,
        u64,
        u64,
    ) {
        let amm_config = AmmConfig {
            trade_fee_rate: 1000,
            tick_spacing,
            ..Default::default()
        };

        let pool_state_refcel = build_pool(
            start_tick,
            tick_spacing,
            tick_math::get_sqrt_price_at_tick(start_tick).unwrap(),
            0,
        );

        let observation_state = RefCell::new(ObservationState::default());

        let bitmap_extension =
            build_tick_array_bitmap_extension_info(pool_state_refcel.borrow().key());
        let mut tick_array_states: VecDeque<RefCell<TickArrayState>> = VecDeque::new();
        let mut sum_amount_0: u64 = 0;
        let mut sum_amount_1: u64 = 0;
        {
            let mut pool_state = pool_state_refcel.borrow_mut();
            observation_state.borrow_mut().pool_id = pool_state.key();

            let mut tick_array_map = HashMap::new();

            for position_param in position_params {
                let liquidity = liquidity_math::get_liquidity_from_amounts(
                    pool_state.sqrt_price_x64,
                    tick_math::get_sqrt_price_at_tick(position_param.tick_lower).unwrap(),
                    tick_math::get_sqrt_price_at_tick(position_param.tick_upper).unwrap(),
                    position_param.amount_0,
                    position_param.amount_1,
                )
                .unwrap();

                let (amount_0, amount_1) = get_delta_amounts_signed(
                    start_tick,
                    tick_math::get_sqrt_price_at_tick(start_tick).unwrap(),
                    position_param.tick_lower,
                    position_param.tick_upper,
                    liquidity as i128,
                )
                .unwrap();
                sum_amount_0 += amount_0;
                sum_amount_1 += amount_1;
                let tick_array_lower_start_index =
                    TickArrayState::get_array_start_index(position_param.tick_lower, tick_spacing);

                if !tick_array_map.contains_key(&tick_array_lower_start_index) {
                    let mut tick_array_refcel = build_tick_array_with_tick_states(
                        pool_state.key(),
                        tick_array_lower_start_index,
                        tick_spacing,
                        vec![],
                    );
                    let tick_array_lower = tick_array_refcel.get_mut();

                    let tick_lower = tick_array_lower
                        .get_tick_state_mut(position_param.tick_lower, tick_spacing)
                        .unwrap();
                    tick_lower.tick = position_param.tick_lower;
                    tick_lower
                        .update(
                            pool_state.tick_current,
                            i128::try_from(liquidity).unwrap(),
                            0,
                            0,
                            false,
                            &[RewardInfo::default(); 3],
                        )
                        .unwrap();

                    tick_array_map.insert(tick_array_lower_start_index, tick_array_refcel);
                } else {
                    let tick_array_lower = tick_array_map
                        .get_mut(&tick_array_lower_start_index)
                        .unwrap();
                    let mut tick_array_lower_borrow_mut = tick_array_lower.borrow_mut();
                    let tick_lower = tick_array_lower_borrow_mut
                        .get_tick_state_mut(position_param.tick_lower, tick_spacing)
                        .unwrap();

                    tick_lower
                        .update(
                            pool_state.tick_current,
                            i128::try_from(liquidity).unwrap(),
                            0,
                            0,
                            false,
                            &[RewardInfo::default(); 3],
                        )
                        .unwrap();
                }
                let tick_array_upper_start_index =
                    TickArrayState::get_array_start_index(position_param.tick_upper, tick_spacing);
                if !tick_array_map.contains_key(&tick_array_upper_start_index) {
                    let mut tick_array_refcel = build_tick_array_with_tick_states(
                        pool_state.key(),
                        tick_array_upper_start_index,
                        tick_spacing,
                        vec![],
                    );
                    let tick_array_upper = tick_array_refcel.get_mut();

                    let tick_upper = tick_array_upper
                        .get_tick_state_mut(position_param.tick_upper, tick_spacing)
                        .unwrap();
                    tick_upper.tick = position_param.tick_upper;

                    tick_upper
                        .update(
                            pool_state.tick_current,
                            i128::try_from(liquidity).unwrap(),
                            0,
                            0,
                            true,
                            &[RewardInfo::default(); 3],
                        )
                        .unwrap();

                    tick_array_map.insert(tick_array_upper_start_index, tick_array_refcel);
                } else {
                    let tick_array_upper = tick_array_map
                        .get_mut(&tick_array_upper_start_index)
                        .unwrap();

                    let mut tick_array_upperr_borrow_mut = tick_array_upper.borrow_mut();
                    let tick_upper = tick_array_upperr_borrow_mut
                        .get_tick_state_mut(position_param.tick_upper, tick_spacing)
                        .unwrap();

                    tick_upper
                        .update(
                            pool_state.tick_current,
                            i128::try_from(liquidity).unwrap(),
                            0,
                            0,
                            true,
                            &[RewardInfo::default(); 3],
                        )
                        .unwrap();
                }
                if pool_state.tick_current >= position_param.tick_lower
                    && pool_state.tick_current < position_param.tick_upper
                {
                    pool_state.liquidity = liquidity_math::add_delta(
                        pool_state.liquidity,
                        i128::try_from(liquidity).unwrap(),
                    )
                    .unwrap();
                }
            }
            for (tickarray_start_index, tick_array_info) in tick_array_map {
                tick_array_states.push_back(tick_array_info);
                pool_state
                    .flip_tick_array_bit(Some(&bitmap_extension), tickarray_start_index)
                    .unwrap();
            }

            use std::convert::identity;
            if zero_for_one {
                tick_array_states.make_contiguous().sort_by(|a, b| {
                    identity(b.borrow().start_tick_index)
                        .cmp(&identity(a.borrow().start_tick_index))
                });
            } else {
                tick_array_states.make_contiguous().sort_by(|a, b| {
                    identity(a.borrow().start_tick_index)
                        .cmp(&identity(b.borrow().start_tick_index))
                });
            }
        }

        (
            amm_config,
            pool_state_refcel,
            tick_array_states,
            observation_state,
            bitmap_extension,
            sum_amount_0,
            sum_amount_1,
        )
    }

    #[cfg(test)]
    mod cross_tick_array_test {
        use super::*;

        #[test]
        fn zero_for_one_base_input_test() {
            let mut tick_current = -32395;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3651942632306380802;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32400), hasn't reached the next tick array initialized tick
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                12188240002,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 277065331032));
            assert!(amount_0 == 12188240002);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32400) in last step, now tickarray_current is the tickarray with start_index -36000,
            // so we pop the tickarray with start_index -32400
            // in this swap we will cross the tick(-32460), but not reach next tick (-32520)
            tick_array_states.pop_front();
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                121882400020,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32520
                    && pool_state.borrow().tick_current < -32460
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 536061033698));
            assert!(amount_0 == 121882400020);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -36000, cross the tick -32520
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                60941200010,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32580
                    && pool_state.borrow().tick_current < -32520
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 790917615645));
            assert!(amount_0 == 60941200010);
        }

        #[test]
        fn zero_for_one_base_output_test() {
            let mut tick_current = -32395;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3651942632306380802;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32400), hasn't reached the next tick array initialized tick
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                477470480,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 277065331032));
            assert!(amount_1 == 477470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32400) in last step, now tickarray_current is the tickarray with start_index -36000,
            // so we pop the tickarray with start_index -32400
            // in this swap we will cross the tick(-32460), but not reach next tick (-32520)
            tick_array_states.pop_front();
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                4751002622,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32520
                    && pool_state.borrow().tick_current < -32460
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 536061033698));
            assert!(amount_1 == 4751002622);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -36000
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                2358130642,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32580
                    && pool_state.borrow().tick_current < -32520
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 790917615645));
            assert!(amount_1 == 2358130642);
        }

        #[test]
        fn one_for_zero_base_input_test() {
            let mut tick_current = -32470;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3638127228312488926;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32460), hasn't reached the next tick array initialized tick
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                887470480,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 536061033698));
            assert!(amount_1 == 887470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32460) in last step, but not reached tick -32400, because -32400 is the next tickarray boundary,
            // so the tickarray_current still is the tick array with start_index -36000
            // in this swap we will cross the tick(-32400), but not reach next tick (-29220)
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                3087470480,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_1 == 3087470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -32400, cross the tick -29220
            tick_array_states.pop_front();
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                200941200010,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 1330680689));
            assert!(amount_1 == 200941200010);
        }

        #[test]
        fn one_for_zero_base_output_test() {
            let mut tick_current = -32470;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3638127228312488926;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32460), hasn't reached the next tick array initialized tick
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                22796232052,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 536061033698));
            assert!(amount_0 == 22796232052);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32460) in last step, but not reached tick -32400, because -32400 is the next tickarray boundary,
            // so the tickarray_current still is the tick array with start_index -36000
            // in this swap we will cross the tick(-32400), but not reach next tick (-29220)
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                79023558189,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_0 == 79023558189);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -32400, cross the tick -29220
            tick_array_states.pop_front();
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                4315086194758,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 1330680689));
            assert!(amount_0 == 4315086194758);
        }
    }

    #[cfg(test)]
    mod find_next_initialized_tick_test {
        use super::*;

        #[test]
        fn zero_for_one_current_tick_array_not_initialized_test() {
            let tick_current = -28776;
            let liquidity = 624165121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![
                        build_tick(-32400, 277065331032, -277065331032).take(),
                        build_tick(-29220, 1330680689, -1330680689).take(),
                        build_tick(-28860, 6408486554, -6408486554).take(),
                    ],
                }],
            );

            // find the first initialized tick(-28860) and cross it in tickarray
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                12188240002,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 6408486554));
            assert!(amount_0 == 12188240002);
        }

        #[test]
        fn one_for_zero_current_tick_array_not_initialized_test() {
            let tick_current = -32405;
            let liquidity = 1224165121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![
                        build_tick(-32400, 277065331032, -277065331032).take(),
                        build_tick(-29220, 1330680689, -1330680689).take(),
                        build_tick(-28860, 6408486554, -6408486554).take(),
                    ],
                }],
            );

            // find the first initialized tick(-32400) and cross it in tickarray
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                12188240002,
                tick_math::get_sqrt_price_at_tick(-28860).unwrap(),
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_1 == 12188240002);
        }
    }

    #[cfg(test)]
    mod liquidity_insufficient_test {
        use super::*;
        use crate::error::ErrorCode;
        #[test]
        fn no_enough_initialized_tickarray_in_pool_test() {
            let tick_current = -28776;
            let liquidity = 121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![build_tick(-28860, 6408486554, -6408486554).take()],
                }],
            );

            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                12188240002,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            );
            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err(),
                ErrorCode::MissingTickArrayBitmapExtensionAccount.into()
            );
        }
    }

    #[test]
    fn explain_why_zero_for_one_less_or_equal_current_tick() {
        let tick_current = -28859;
        let mut liquidity = 121219;
        let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
        let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
            tick_current,
            60,
            sqrt_price_x64,
            liquidity,
            vec![TickArrayInfo {
                start_tick_index: -32400,
                ticks: vec![
                    build_tick(-32400, 277065331032, -277065331032).take(),
                    build_tick(-29220, 1330680689, -1330680689).take(),
                    build_tick(-28860, 6408486554, -6408486554).take(),
                ],
            }],
        );

        // not cross tick(-28860), but pool.tick_current = -28860
        let SwapInternalResult {
            amount_0, amount_1, ..
        } = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            None,
            25,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current < tick_current);
        assert!(pool_state.borrow().tick_current == -28860);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28860).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity);
        assert!(amount_0 == 25);

        // just cross tick(-28860), pool.tick_current = -28861
        let SwapInternalResult {
            amount_0, amount_1, ..
        } = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            None,
            3,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current < tick_current);
        assert!(pool_state.borrow().tick_current == -28861);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity + 6408486554);
        assert!(amount_0 == 3);

        liquidity = pool_state.borrow().liquidity;

        // we swap just a little amount, let pool tick_current also equal -28861
        // but pool.sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861)
        let SwapInternalResult {
            amount_0, amount_1, ..
        } = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            None,
            50,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current == -28861);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity);
        assert!(amount_0 == 50);
    }

    #[cfg(test)]
    mod swap_edge_test {
        use super::*;

        #[test]
        fn zero_for_one_swap_edge_case() {
            let mut tick_current = -28859;
            let liquidity = 121219;
            let mut sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![
                    TickArrayInfo {
                        start_tick_index: -32400,
                        ticks: vec![
                            build_tick(-32400, 277065331032, -277065331032).take(),
                            build_tick(-29220, 1330680689, -1330680689).take(),
                            build_tick(-28860, 6408486554, -6408486554).take(),
                        ],
                    },
                    TickArrayInfo {
                        start_tick_index: -28800,
                        ticks: vec![build_tick(-28800, 3726362727, -3726362727).take()],
                    },
                ],
            );

            // zero for one, just cross tick(-28860),  pool.tick_current = -28861 and pool.sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(-28860)
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                27,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(pool_state.borrow().tick_current == -28861);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(-28860).unwrap()
            );
            assert!(pool_state.borrow().liquidity == liquidity + 6408486554);
            assert!(amount_0 == 27);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;

            // we swap just a little amount, it is completely taken by fees, the sqrt price and the tick will remain the same
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                1,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current == tick_current);
            assert!(pool_state.borrow().tick_current == -28861);
            assert!(pool_state.borrow().sqrt_price_x64 == sqrt_price_x64);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;

            // reverse swap direction, one_for_zero
            // Actually, the loop for this swap was executed twice because the previous swap happened to have `pool.tick_current` exactly on the boundary that is divisible by `tick_spacing`.
            // In the first iteration of this swap's loop, it found the initial tick (-28860), but at this point, both the initial and final prices were equal to the price at tick -28860.
            // This did not meet the conditions for swapping so both swap_amount_input and swap_amount_output were 0. The actual output was calculated in the second iteration of the loop.
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                10,
                tick_math::get_sqrt_price_at_tick(-28800).unwrap(),
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(
                pool_state.borrow().tick_current > -28860
                    && pool_state.borrow().tick_current <= -28800
            );
        }
    }

    #[cfg(test)]
    mod sqrt_price_limit_optimization_min_specified_test {
        use super::*;
        #[test]
        fn zero_for_one_base_input_with_min_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = true;
            let is_base_input = true;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX - 1;
            let amount_1 = u64::MAX - 1;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = 1;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }

        #[test]
        fn zero_for_one_base_out_with_min_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = true;
            let is_base_input = false;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX - 1;
            let amount_1 = u64::MAX - 1;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = 1;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }

        #[test]
        fn one_for_zero_base_in_with_min_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = false;
            let is_base_input = true;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX - 1;
            let amount_1 = u64::MAX - 1;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = 1;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }
        #[test]
        fn one_for_zero_base_out_with_min_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = false;
            let is_base_input = false;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX - 1;
            let amount_1 = u64::MAX - 1;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = 1;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }
    }
    #[cfg(test)]
    mod sqrt_price_limit_optimization_max_specified_test {
        use super::*;
        #[test]
        fn zero_for_one_base_input_with_max_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = true;
            let is_base_input = true;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX / 2;
            let amount_1 = u64::MAX / 2;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = u64::MAX / 2;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }

        #[test]
        fn zero_for_one_base_out_with_max_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = true;
            let is_base_input = false;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX / 2;
            let amount_1 = u64::MAX / 2;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = u64::MAX / 4;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }

        #[test]
        fn one_for_zero_base_in_with_max_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = false;
            let is_base_input = true;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX / 2;
            let amount_1 = u64::MAX / 2;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = u64::MAX / 2;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }
        #[test]
        fn one_for_zero_base_out_with_min_amount_specified() {
            let tick_spacing = 10;
            let zero_for_one = false;
            let is_base_input = false;
            let tick_lower = tick_math::MIN_TICK + 1;
            let tick_upper = tick_math::MAX_TICK - 1;
            let tick_current = 0;
            let amount_0 = u64::MAX / 2;
            let amount_1 = u64::MAX / 2;

            let (
                amm_config,
                pool_state,
                tick_array_states,
                observation_state,
                bitmap_extension_info,
                sum_amount_0,
                sum_amount_1,
            ) = setup_swap_test(
                tick_current,
                tick_spacing as u16,
                vec![OpenPositionParam {
                    amount_0: amount_0,
                    amount_1: amount_1,
                    tick_lower: tick_lower,
                    tick_upper: tick_upper,
                }],
                zero_for_one,
            );
            println!(
                "sum_amount_0: {}, sum_amount_1: {}",
                sum_amount_0, sum_amount_1,
            );
            let amount_specified = u64::MAX / 4;
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                Some(bitmap_extension_info),
                amount_specified,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                zero_for_one,
                is_base_input,
                1,
            );
            println!("{:#?}", result);
            let pool = pool_state.borrow();
            let sqrt_price_x64 = pool.sqrt_price_x64;
            let sqrt_price = sqrt_price_x64 as f64 / fixed_point_64::Q64 as f64;
            println!("price: {}", sqrt_price * sqrt_price);
        }
    }
    #[cfg(test)]
    mod sqrt_price_limit_optimization_test {
        use super::*;
        use proptest::prelude::*;
        use std::{convert::identity, u64};

        use proptest::prop_assume;
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(2048))]

            #[test]
            fn zero_for_one_base_input_test(
                tick_current in tick_math::MIN_TICK..tick_math::MAX_TICK,
                amount_0 in 1000000..u64::MAX,
                amount_1 in 1000000..u64::MAX,
                tick_lower in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 10", |x| x % 10 == 0),
                tick_upper in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 10", |x| x % 10 == 0),
            ){
                let tick_spacing = 10;
                let zero_for_one = true;
                let is_base_input = true;
                if tick_lower%tick_spacing ==0 && tick_upper%tick_spacing ==0 && tick_upper>tick_lower{

                    let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  sum_amount_0, sum_amount_1) = setup_swap_test(
                        tick_current,
                        tick_spacing as u16,
                        vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                        zero_for_one
                        );

                    prop_assume!(sum_amount_1 > 1);
                    let mut rng = rand::thread_rng();
                    let amount_specified  = rng.gen_range(1..u64::MAX - sum_amount_0);

                    let result = swap_internal(
                        &amm_config,
                        &mut pool_state.borrow_mut(),
                        &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                        &mut observation_state.borrow_mut(),
                        Some(bitmap_extension_info),
                        amount_specified,
                        tick_math::MIN_SQRT_PRICE_X64 + 1,
                        zero_for_one,
                        is_base_input,
                        0,
                    );

                    if result.is_ok() {
                        let SwapInternalResult { amount_0: amount_0_before, amount_1: amount_1_before, .. } = result.unwrap();

                        let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  _sum_amount_0, _sum_amount_1) = setup_swap_test(
                            tick_current,
                            tick_spacing as u16,
                            vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                            zero_for_one
                        );
                        let result = swap_internal(
                            &amm_config,
                            &mut pool_state.borrow_mut(),
                            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                            &mut observation_state.borrow_mut(),
                            Some(bitmap_extension_info),
                            amount_specified,
                            tick_math::MIN_SQRT_PRICE_X64 + 1,
                            zero_for_one,
                            is_base_input,
                            oracle::block_timestamp_mock() as u32,
                        );
                        assert!(result.is_ok());

                        // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{},liquidity:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper, identity(pool_state.borrow().liquidity));

                        let SwapInternalResult { amount_0: amount_0_after, amount_1: amount_1_after, .. } = result.unwrap();
                        assert_eq!(amount_0_before, amount_0_after);
                        assert_eq!(amount_1_before, amount_1_after);

                    }else{
                        let err =  result.err().unwrap();
                        if err == crate::error::ErrorCode::MaxTokenOverflow.into(){
                            println!("##### original swap is overflow ");
                            let result = swap_internal(
                                &amm_config,
                                &mut pool_state.borrow_mut(),
                                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                                &mut observation_state.borrow_mut(),
                                Some(bitmap_extension_info),
                                amount_specified,
                                tick_math::MIN_SQRT_PRICE_X64 + 1,
                                zero_for_one,
                                is_base_input,
                                oracle::block_timestamp_mock() as u32,
                            );
                            if result.is_err(){
                                println!("{:#?}", result);
                            }
                        }else{
                            println!("{}", err);
                        }
                    }
                }
            }

            #[test]
            fn zero_for_one_base_output_test(
                tick_current in tick_math::MIN_TICK..tick_math::MAX_TICK,
                amount_0 in 1000000..u64::MAX,
                amount_1 in 1000000..u64::MAX,
                tick_lower in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
                tick_upper in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
            ){
                let tick_spacing = 10;
                let zero_for_one = true;
                let base_input= false;
                if tick_lower%tick_spacing ==0 && tick_upper%tick_spacing ==0 && tick_upper>tick_lower{
                    let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info, _sum_amount_0, sum_amount_1) = setup_swap_test(
                        tick_current,
                        tick_spacing as u16,
                        vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                        zero_for_one
                    );

                    prop_assume!(sum_amount_1 > 1);
                    let mut rng = rand::thread_rng();
                    let amount_specified  = rng.gen_range(1..sum_amount_1);
                    // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper);
                    let result = swap_internal(
                        &amm_config,
                        &mut pool_state.borrow_mut(),
                        &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                        &mut observation_state.borrow_mut(),
                        Some(bitmap_extension_info),
                        amount_specified,
                        tick_math::MIN_SQRT_PRICE_X64 + 1,
                        zero_for_one,
                        base_input,
                        0,
                    );

                    if result.is_ok() {
                        let SwapInternalResult { amount_0: amount_0_before, amount_1: amount_1_before, .. } = result.unwrap();

                        let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info, _sum_amount_0, _sum_amount_1) = setup_swap_test(
                            tick_current,
                            tick_spacing as u16,
                            vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                            zero_for_one
                        );
                        let result = swap_internal(
                            &amm_config,
                            &mut pool_state.borrow_mut(),
                            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                            &mut observation_state.borrow_mut(),
                            Some(bitmap_extension_info),
                            amount_specified,
                            tick_math::MIN_SQRT_PRICE_X64 + 1,
                            zero_for_one,
                            base_input,
                            oracle::block_timestamp_mock() as u32,
                        );
                        assert!(result.is_ok());

                        println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{},liquidity:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper, identity(pool_state.borrow().liquidity));

                        let SwapInternalResult { amount_0: amount_0_after, amount_1: amount_1_after, .. } = result.unwrap();
                        assert_eq!(amount_0_before, amount_0_after);
                        assert_eq!(amount_1_before, amount_1_after);

                    }else{
                        let err =  result.err().unwrap();
                        if err == crate::error::ErrorCode::MaxTokenOverflow.into(){
                            println!("##### original swap is overflow");
                            let result = swap_internal(
                                &amm_config,
                                &mut pool_state.borrow_mut(),
                                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                                &mut observation_state.borrow_mut(),
                                Some(bitmap_extension_info),
                                amount_specified,
                                tick_math::MIN_SQRT_PRICE_X64 + 1,
                                zero_for_one,
                                base_input,
                                oracle::block_timestamp_mock() as u32,
                            );
                            if result.is_err(){
                                println!("{:#?}", result);
                            }
                        }else{
                            println!("{}", err);
                        }
                    }
                }
            }

            #[test]
            fn one_for_zero_base_input_test(
                tick_current in tick_math::MIN_TICK..tick_math::MAX_TICK,
                amount_0 in 1000000..u64::MAX,
                amount_1 in 1000000..u64::MAX,
                tick_lower in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
                tick_upper in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
            ){
                let tick_spacing = 10;
                let zero_for_one = false;
                let is_base_input = true;
                if tick_lower%tick_spacing ==0 && tick_upper%tick_spacing ==0 && tick_current>tick_lower && tick_current<tick_upper{
                    // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper);
                    let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  sum_amount_0, sum_amount_1) = setup_swap_test(
                        tick_current,
                        tick_spacing as u16,
                        vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                        zero_for_one
                    );

                    prop_assume!(sum_amount_0 > 1);
                    let mut rng = rand::thread_rng();
                    let amount_specified  = rng.gen_range(1..u64::MAX - sum_amount_1);

                    let result = swap_internal(
                        &amm_config,
                        &mut pool_state.borrow_mut(),
                        &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                        &mut observation_state.borrow_mut(),
                        Some(bitmap_extension_info),
                        amount_specified,
                        tick_math::MAX_SQRT_PRICE_X64 - 1,
                        zero_for_one,
                        is_base_input,
                        0,
                    );

                    if result.is_ok() {
                        let SwapInternalResult { amount_0: amount_0_before, amount_1: amount_1_before, .. } = result.unwrap();

                        let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  _sum_amount_0, _sum_amount_1) = setup_swap_test(
                            tick_current,
                            tick_spacing as u16,
                            vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                            zero_for_one
                        );
                        let result = swap_internal(
                            &amm_config,
                            &mut pool_state.borrow_mut(),
                            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                            &mut observation_state.borrow_mut(),
                            Some(bitmap_extension_info),
                            amount_specified,
                            tick_math::MAX_SQRT_PRICE_X64 - 1,
                            zero_for_one,
                            is_base_input,
                            oracle::block_timestamp_mock() as u32,
                        );
                        assert!(result.is_ok());

                        // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{},liquidity:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper, identity(pool_state.borrow().liquidity));

                        let SwapInternalResult { amount_0: amount_0_after, amount_1: amount_1_after, .. } = result.unwrap();
                        assert_eq!(amount_0_before, amount_0_after);
                        assert_eq!(amount_1_before, amount_1_after);

                    }else {
                        let err =  result.err().unwrap();
                        if err == crate::error::ErrorCode::MaxTokenOverflow.into(){
                            // println!("##### original swap is overflow ");
                            let _result = swap_internal(
                                &amm_config,
                                &mut pool_state.borrow_mut(),
                                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                                &mut observation_state.borrow_mut(),
                                Some(bitmap_extension_info),
                                amount_specified,
                                tick_math::MAX_SQRT_PRICE_X64 - 1,
                                zero_for_one,
                                is_base_input,
                                oracle::block_timestamp_mock() as u32,
                            );

                        }else{
                            println!("{}", err);
                        }
                    }
                }
            }

            #[test]
            fn one_for_zero_base_output_test(
                tick_current in tick_math::MIN_TICK..tick_math::MAX_TICK,
                amount_0 in 1000000..u64::MAX,
                amount_1 in 1000000..u64::MAX,
                tick_lower in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
                tick_upper in (tick_math::MIN_TICK..=tick_math::MAX_TICK).prop_filter("Must be multiple of 100", |x| x % 10 == 0),
            ){
                let tick_spacing = 10;
                let zero_for_one = false;
                let is_base_input = false;
                if tick_lower%tick_spacing ==0 && tick_upper%tick_spacing ==0 && tick_current>tick_lower && tick_current<tick_upper{

                    // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper);
                    let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  sum_amount_0, _sum_amount_1) = setup_swap_test(
                        tick_current,
                        tick_spacing as u16,
                        vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                        zero_for_one
                    );
                    prop_assume!(sum_amount_0 > 1);
                    let mut rng = rand::thread_rng();
                    let amount_specified  = rng.gen_range(1..sum_amount_0);

                    let result = swap_internal(
                        &amm_config,
                        &mut pool_state.borrow_mut(),
                        &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                        &mut observation_state.borrow_mut(),
                        Some(bitmap_extension_info),
                        amount_specified,
                        tick_math::MAX_SQRT_PRICE_X64 - 1,
                        zero_for_one,
                        is_base_input,
                        0,
                    );

                    if result.is_ok() {
                        let SwapInternalResult { amount_0: amount_0_before, amount_1: amount_1_before, .. } = result.unwrap();

                        let (amm_config, pool_state, tick_array_states, observation_state,bitmap_extension_info,  _sum_amount_0, _sum_amount_1) = setup_swap_test(
                            tick_current,
                            tick_spacing as u16,
                            vec![OpenPositionParam{amount_0:amount_0,amount_1:amount_1, tick_lower:tick_lower, tick_upper:tick_upper}],
                            zero_for_one
                        );
                        let result = swap_internal(
                            &amm_config,
                            &mut pool_state.borrow_mut(),
                            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                            &mut observation_state.borrow_mut(),
                            Some(bitmap_extension_info),
                            amount_specified,
                            tick_math::MAX_SQRT_PRICE_X64 - 1,
                            zero_for_one,
                            is_base_input,
                            oracle::block_timestamp_mock() as u32,
                        );
                        assert!(result.is_ok());

                        // println!("----- input: tick_current:{}, amount_0:{}, amount_1:{}, amount_specified:{},tick_lower:{}, tick_upper:{},liquidity:{}", tick_current, amount_0, amount_1,amount_specified, tick_lower, tick_upper, identity(pool_state.borrow().liquidity));

                        let SwapInternalResult { amount_0: amount_0_after, amount_1: amount_1_after, .. } = result.unwrap();
                        assert_eq!(amount_0_before, amount_0_after);
                        assert_eq!(amount_1_before, amount_1_after);

                    }else {
                        let err =  result.err().unwrap();
                        if err == crate::error::ErrorCode::MaxTokenOverflow.into(){
                            println!("##### original swap is overflow ");
                            let _result = swap_internal(
                                &amm_config,
                                &mut pool_state.borrow_mut(),
                                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                                &mut observation_state.borrow_mut(),
                                Some(bitmap_extension_info),
                                amount_specified,
                                tick_math::MAX_SQRT_PRICE_X64 - 1,
                                zero_for_one,
                                is_base_input,
                                oracle::block_timestamp_mock() as u32,
                            );
                        }else{
                            println!("{}", err);
                        }
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod limit_order_swap_test {
        use super::*;

        /// Get the unfilled limit order amount for a specific tick
        fn get_tick_state(
            tick_array_states: &VecDeque<RefCell<TickArrayState>>,
            tick_index: i32,
        ) -> Option<TickState> {
            for tick_array in tick_array_states.iter() {
                for tick in tick_array.borrow().ticks.iter() {
                    if tick.tick == tick_index {
                        return Some(tick.clone());
                    }
                }
            }
            None
        }
        /// Tests for tick assignment and cross tick with limit orders
        mod base_input_tick_assignment_and_cross_with_limit_orders_test {
            use super::*;

            /// Test: Reach tick with limit order, then partially execute it, then fully execute and cross tick
            /// Flow: tick=0 → tick=-10 (no execution) → partial execution → full execution + cross tick=-10
            #[test]
            fn test_zero_for_one_tick_assignment_with_limit_order() {
                let is_base_input = true;
                let tick_current = 0;
                let liquidity = 5124165121219;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
                let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
                tick_with_limit_order1.orders_amount = 1000000;
                let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
                tick_with_limit_order2.orders_amount = 1000000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_swap_param(
                        tick_current,
                        1,
                        sqrt_price_x64,
                        liquidity,
                        vec![
                            TickArrayInfo {
                                start_tick_index: 0,
                                ticks: vec![
                                    tick_with_limit_order2,
                                    build_tick(20, 790917615645, 0).take(),
                                ],
                            },
                            TickArrayInfo {
                                start_tick_index: -60,
                                ticks: vec![
                                    build_tick(-20, 1330680689, -1330680689).take(),
                                    tick_with_limit_order1,
                                ],
                            },
                        ],
                    );
                let tick_state = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();
                // first swap, reach tick(-10), no limit order executed
                let SwapInternalResult { .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    2565160190,
                    tick_math::get_sqrt_price_at_tick(-10).unwrap(),
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let mut tick_state1 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1, unfilled_amount);

                let result_expected = tick_state1
                    .match_limit_order(1000, true, !is_base_input, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_in = result_expected.amount_in + result_expected.amm_fee_amount;

                // continue swap, partial execution of the limit order
                let SwapInternalResult {
                    amount_0, amount_1, ..
                } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_in,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_1, 1000);
                assert_eq!(amount_0, amount_in);
                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let mut tick_state2 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1 - unfilled_amount2, 1000);

                let result_expected = tick_state2
                    .match_limit_order(
                        unfilled_amount2,
                        true,
                        !is_base_input,
                        amm_config.trade_fee_rate,
                        true,
                    )
                    .unwrap();
                let amount_in = result_expected.amount_in + result_expected.amm_fee_amount;
                // continue swap,  all limit orders are executed, just cross tick(-10)
                let SwapInternalResult {
                    amount_0, amount_1, ..
                } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_in,
                    tick_math::get_sqrt_price_at_tick(-11).unwrap(),
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, amount_in);
                assert_eq!(amount_1, unfilled_amount2);
                assert!(pool_state.borrow().tick_current == -11);
                assert!(
                    pool_state.borrow().liquidity
                        == liquidity_math::add_delta(liquidity, tick_state.liquidity_net.neg())
                            .unwrap()
                );
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                let tick_state_after = get_tick_state(&tick_array_states, -10).unwrap();
                assert_eq!(tick_state_after.limit_order_unfilled_amount().unwrap(), 0);
            }

            /// Test: Reach tick with limit order (opposite direction), then partially execute it, then fully execute and cross tick
            /// Flow: tick=0 → tick=10 (no execution) → partial execution → full execution + cross tick=10
            #[test]
            fn test_one_for_zero_tick_assignment_with_limit_order() {
                let is_base_input = true;
                let tick_current = 0;
                let liquidity = 5124165121219;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
                let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
                tick_with_limit_order1.orders_amount = 1000000;
                let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
                tick_with_limit_order2.orders_amount = 1000000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_swap_param(
                        tick_current,
                        1,
                        sqrt_price_x64,
                        liquidity,
                        vec![
                            TickArrayInfo {
                                start_tick_index: -60,
                                ticks: vec![
                                    build_tick(-20, 1330680689, -1330680689).take(),
                                    tick_with_limit_order1,
                                ],
                            },
                            TickArrayInfo {
                                start_tick_index: 0,
                                ticks: vec![
                                    tick_with_limit_order2,
                                    build_tick(20, 790917615645, 0).take(),
                                ],
                            },
                        ],
                    );

                let tick_state = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();

                // First swap, reach tick(10), no limit order executed
                let SwapInternalResult { .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    2565160190,
                    tick_math::get_sqrt_price_at_tick(10).unwrap(),
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert!(pool_state.borrow().tick_current == 9);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);

                let mut tick_state1 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1, unfilled_amount);

                let result_expected = tick_state1
                    .match_limit_order(1000, false, !is_base_input, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_in = result_expected.amount_in + result_expected.amm_fee_amount;
                // continue swap, partial execution of the limit order
                let SwapInternalResult {
                    amount_0, amount_1, ..
                } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_in,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 1000);
                assert_eq!(amount_1, amount_in);
                assert!(pool_state.borrow().tick_current == 9);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);

                let mut tick_state2 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1 - unfilled_amount2, 1000);

                let result_expected = tick_state2
                    .match_limit_order(
                        unfilled_amount2,
                        false,
                        !is_base_input,
                        amm_config.trade_fee_rate,
                        true,
                    )
                    .unwrap();
                let amount_in = result_expected.amount_in + result_expected.amm_fee_amount;

                // continue swap, just cross tick(-10), full execution of the limit order
                let SwapInternalResult {
                    amount_0, amount_1, ..
                } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_in,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                println!("amount_0:{},amount_1:{}", amount_0, amount_1);
                assert!(pool_state.borrow().tick_current == 10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(
                    pool_state.borrow().liquidity
                        == liquidity_math::add_delta(liquidity, tick_state2.liquidity_net).unwrap()
                );

                let tick_state3 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount3 = tick_state3.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount3, 0);
            }

            /// Test: Tick assignment correctness during price oscillation without limit order execution
            /// Flow: tick=0 → tick=-10 (no execution) → tick=10 (no execution) → tick=-10 (no execution)
            /// Verify: Tick assignment remains correct when price oscillates around limit orders without executing them
            #[test]
            fn test_tick_assignment_during_oscillation_without_execution() {
                let is_base_input = true;
                let tick_current = 0;
                let liquidity = 5124165121219;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
                let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
                tick_with_limit_order1.orders_amount = 1000000;
                let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
                tick_with_limit_order2.orders_amount = 1000000;

                let (amm_config, pool_state, mut tick_array_states, observation_state) =
                    build_swap_param(
                        tick_current,
                        1,
                        sqrt_price_x64,
                        liquidity,
                        vec![
                            TickArrayInfo {
                                start_tick_index: 0,
                                ticks: vec![
                                    tick_with_limit_order2,
                                    build_tick(20, 790917615645, 0).take(),
                                ],
                            },
                            TickArrayInfo {
                                start_tick_index: -60,
                                ticks: vec![
                                    build_tick(-20, 1330680689, -1330680689).take(),
                                    tick_with_limit_order1,
                                ],
                            },
                        ],
                    );
                let tick_state = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();
                // first swap, reach tick(-10), no limit order executed
                let SwapInternalResult { .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    2565160190,
                    tick_math::get_sqrt_price_at_tick(-10).unwrap(),
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state1 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1, unfilled_amount);

                // Reverse swap, reach tick(10), no limit order executed
                let tick_state2 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();

                tick_array_states.make_contiguous().reverse();
                let SwapInternalResult { amount_1, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_1, 5129038183);
                assert!(pool_state.borrow().tick_current == 9);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state3 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount3 = tick_state3.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount3, unfilled_amount2);

                // Second reverse swap, reach tick(-10), no limit order executed
                tick_array_states.make_contiguous().reverse();
                let tick_state4 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount4 = tick_state4.limit_order_unfilled_amount().unwrap();

                let SwapInternalResult { amount_0, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 5129038183);
                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state5 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount5 = tick_state5.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount5, unfilled_amount4);
            }

            /// Test: Tick assignment correctness after partial execution and during oscillation
            /// Flow: tick=0 → tick=-10 (partial execution) → tick=10 (partial execution) → tick=-10 (no execution)
            /// Verify: Tick assignment remains correct after partial execution and during price oscillation
            #[test]
            fn test_tick_assignment_after_partial_execution_and_oscillation() {
                let is_base_input = true;
                let tick_current = 0;
                let liquidity = 5124165121219;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
                let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
                tick_with_limit_order1.orders_amount = 1000000;
                let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
                tick_with_limit_order2.orders_amount = 1000000;

                let (amm_config, pool_state, mut tick_array_states, observation_state) =
                    build_swap_param(
                        tick_current,
                        1,
                        sqrt_price_x64,
                        liquidity,
                        vec![
                            TickArrayInfo {
                                start_tick_index: 0,
                                ticks: vec![
                                    tick_with_limit_order2,
                                    build_tick(20, 790917615645, 0).take(),
                                ],
                            },
                            TickArrayInfo {
                                start_tick_index: -60,
                                ticks: vec![
                                    build_tick(-20, 1330680689, -1330680689).take(),
                                    tick_with_limit_order1,
                                ],
                            },
                        ],
                    );
                let mut tick_state = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();
                let result_expected = tick_state
                    .match_limit_order(1000, true, !is_base_input, amm_config.trade_fee_rate, true)
                    .unwrap();
                let expected_amount_in = result_expected.amount_in + result_expected.amm_fee_amount;
                // first swap, reach tick(-10), partial execution of the limit order
                let SwapInternalResult { amount_0, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    2565160190 + expected_amount_in,
                    tick_math::get_sqrt_price_at_tick(-10).unwrap(),
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 2565160190 + expected_amount_in);
                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state1 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount - unfilled_amount1, 1000);

                // Reverse swap, reach tick(10), partial execution of the limit order
                let tick_state2 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();
                let result_expected = tick_state
                    .match_limit_order(1000, false, !is_base_input, amm_config.trade_fee_rate, true)
                    .unwrap();
                let expected_amount_in = result_expected.amount_in + result_expected.amm_fee_amount;
                tick_array_states.make_contiguous().reverse();
                let SwapInternalResult { amount_1, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183 + expected_amount_in,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_1, 5129038183 + expected_amount_in);
                assert!(pool_state.borrow().tick_current == 9);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state3 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount3 = tick_state3.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount2 - unfilled_amount3, 999); // There is a small offset, ideally this should be exactly 1000

                // Second reverse swap, reach tick(-10), no limit order executed
                tick_array_states.make_contiguous().reverse();
                let tick_state4 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount4 = tick_state4.limit_order_unfilled_amount().unwrap();

                let SwapInternalResult { amount_0, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 5129038183);
                assert!(pool_state.borrow().tick_current == -10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(pool_state.borrow().liquidity == liquidity);
                let tick_state5 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount5 = tick_state5.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount5, unfilled_amount4);
            }

            /// Test: Tick assignment correctness after full execution and crossing empty ticks
            /// Flow: tick=0 → tick=-11 (full execution + cross, tick_current = -11) → tick=10 (full execution + cross, tick_current = 10) → tick=-11 (cross only, tick_current = -11)
            /// Verify: Tick assignment remains correct after full execution and when crossing ticks without limit orders
            #[test]
            fn test_tick_assignment_after_full_execution_and_crossing_empty_ticks() {
                let is_base_input = true;
                let tick_current = 0;
                let liquidity = 5124165121219;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
                let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
                tick_with_limit_order1.orders_amount = 1000000;
                let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
                tick_with_limit_order2.orders_amount = 1000000;

                let (amm_config, pool_state, mut tick_array_states, observation_state) =
                    build_swap_param(
                        tick_current,
                        1,
                        sqrt_price_x64,
                        liquidity,
                        vec![
                            TickArrayInfo {
                                start_tick_index: 0,
                                ticks: vec![
                                    tick_with_limit_order2,
                                    build_tick(20, 790917615645, 0).take(),
                                ],
                            },
                            TickArrayInfo {
                                start_tick_index: -60,
                                ticks: vec![
                                    build_tick(-20, 1330680689, -1330680689).take(),
                                    tick_with_limit_order1,
                                ],
                            },
                        ],
                    );
                let mut tick_state = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();
                let result_expected = tick_state
                    .match_limit_order(
                        unfilled_amount,
                        true,
                        !is_base_input,
                        amm_config.trade_fee_rate,
                        true,
                    )
                    .unwrap();
                let expected_amount_in = result_expected.amount_in + result_expected.amm_fee_amount;
                // first swap, reach tick(-10), full execution of the limit order
                let SwapInternalResult { amount_0, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    2565160190 + expected_amount_in,
                    tick_math::get_sqrt_price_at_tick(-10).unwrap(),
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 2565160190 + expected_amount_in);
                assert!(pool_state.borrow().tick_current == -11);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(
                    pool_state.borrow().liquidity
                        == liquidity_math::add_delta(liquidity, tick_state.liquidity_net.neg())
                            .unwrap()
                );
                let tick_state1 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount1, 0);

                // Reverse swap, reach tick(10), full execution of the limit order
                tick_array_states.make_contiguous().reverse();
                let mut tick_state2 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();
                let result_expected = tick_state2
                    .match_limit_order(
                        unfilled_amount2,
                        false,
                        !is_base_input,
                        amm_config.trade_fee_rate,
                        true,
                    )
                    .unwrap();
                let expected_amount_in = result_expected.amount_in + result_expected.amm_fee_amount;

                let SwapInternalResult { amount_1, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183 + expected_amount_in,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_1, 5129038183 + expected_amount_in);
                assert!(pool_state.borrow().tick_current == 10);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(10).unwrap()
                );
                assert!(
                    pool_state.borrow().liquidity
                        == liquidity_math::add_delta(liquidity, tick_state2.liquidity_net).unwrap()
                );
                let tick_state3 = get_tick_state(&tick_array_states, 10).unwrap();
                let unfilled_amount3 = tick_state3.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount3, 0);

                // Second reverse swap, just cross tick(-10)
                tick_array_states.make_contiguous().reverse();
                let tick_state4 = get_tick_state(&tick_array_states, -10).unwrap();
                let unfilled_amount4 = tick_state4.limit_order_unfilled_amount().unwrap();
                assert_eq!(unfilled_amount4, 0);
                let SwapInternalResult { amount_0, .. } = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    5129038183,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    is_base_input,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();
                assert_eq!(amount_0, 5129038183);
                assert!(pool_state.borrow().tick_current == -11);
                assert!(
                    pool_state.borrow().sqrt_price_x64
                        == tick_math::get_sqrt_price_at_tick(-10).unwrap()
                );
                assert!(
                    pool_state.borrow().liquidity
                        == liquidity_math::add_delta(liquidity, tick_state4.liquidity_net.neg())
                            .unwrap()
                );
            }
        }

        /// Test: Base output mode, both directions partial and full execution
        /// Flow: zero_for_one: tick=0 → tick=-10 (partial 1000) → cross tick=-10 (full execution)
        ///       one_for_zero: tick=-11 → tick=9 (partial 1000) → cross tick=10 (full execution)
        /// Verify: Both directions correctly handle partial and full limit order execution in base output mode
        #[test]
        fn test_base_output_both_directions_partial_then_full_execution() {
            let is_base_input = false;
            let tick_current = 0;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let mut tick_with_limit_order1 = build_tick(-10, 6408486554, -6408486554).take();
            tick_with_limit_order1.orders_amount = 1000000;
            let mut tick_with_limit_order2 = build_tick(10, 0, 0).take();
            tick_with_limit_order2.orders_amount = 1000000;

            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    1,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: 0,
                            ticks: vec![
                                tick_with_limit_order2,
                                build_tick(20, 790917615645, 0).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -60,
                            ticks: vec![
                                build_tick(-20, 1330680689, -1330680689).take(),
                                tick_with_limit_order1,
                            ],
                        },
                    ],
                );
            // Reach tick(-10), partial execution of the limit order
            let SwapInternalResult { amount_1, .. } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                2561314115 + 1000,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                true,
                is_base_input,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            assert_eq!(amount_1, 2561314115 + 1000);
            assert!(pool_state.borrow().tick_current == -10);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(-10).unwrap()
            );
            assert!(pool_state.borrow().liquidity == liquidity);
            let tick_state = get_tick_state(&tick_array_states, -10).unwrap();
            let unfilled_amount = tick_state.limit_order_unfilled_amount().unwrap();
            assert_eq!(
                tick_with_limit_order1
                    .limit_order_unfilled_amount()
                    .unwrap()
                    - unfilled_amount,
                1000
            );
            // cross tick(-10), full execution of the limit order
            let SwapInternalResult { amount_1, .. } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                unfilled_amount,
                tick_math::MIN_SQRT_PRICE_X64 + 1,
                true,
                is_base_input,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            assert_eq!(amount_1, unfilled_amount);
            assert!(pool_state.borrow().tick_current == -11);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(-10).unwrap()
            );
            assert!(
                pool_state.borrow().liquidity
                    == liquidity_math::add_delta(liquidity, tick_state.liquidity_net.neg())
                        .unwrap()
            );

            // Reverse swap, reach tick(10), partially execute limit orders
            tick_array_states.make_contiguous().reverse();
            let tick_state1 = get_tick_state(&tick_array_states, 10).unwrap();
            let unfilled_amount1 = tick_state1.limit_order_unfilled_amount().unwrap();

            let SwapInternalResult { amount_0, .. } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                5123909143 + 1000,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                false,
                is_base_input,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            assert_eq!(amount_0, 5123909143 + 1000);
            assert!(pool_state.borrow().tick_current == 9);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(10).unwrap()
            );
            assert!(pool_state.borrow().liquidity == liquidity);
            let tick_state2 = get_tick_state(&tick_array_states, 10).unwrap();
            let unfilled_amount2 = tick_state2.limit_order_unfilled_amount().unwrap();
            assert_eq!(unfilled_amount1 - unfilled_amount2, 1000);

            // cross tick(10), full execution of the limit order
            tick_array_states.make_contiguous().reverse();

            let SwapInternalResult { amount_0, .. } = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                unfilled_amount2,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                false,
                is_base_input,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            assert_eq!(amount_0, unfilled_amount2);
            assert!(pool_state.borrow().tick_current == 10);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(10).unwrap()
            );
            assert!(pool_state.borrow().liquidity == liquidity);
            let tick_state3 = get_tick_state(&tick_array_states, 10).unwrap();
            let unfilled_amount3 = tick_state3.limit_order_unfilled_amount().unwrap();
            assert_eq!(unfilled_amount3, 0);
        }

        /// This test proves the invariant at lines 660-664:
        /// If a limit order has been executed (unfilled amount changed) AND
        /// the swap still has remaining amount (amount_specified_remaining != 0),
        /// then the limit order must be fully consumed (unfilled_amount_after == 0).
        ///
        /// This is because the swap loop only exits when:
        /// - amount_specified_remaining == 0 (fully consumed), OR
        /// - sqrt_price_x64 == target_price (price limit reached)
        ///
        /// If a limit order was partially filled and we still have remaining swap amount,
        /// the swap would continue to the next tick. Therefore, if we exit the loop with
        /// remaining amount and the limit order was executed, it must have been fully consumed.
        #[test]
        fn test_limit_order_full_consumption_invariant() {
            let tick_current = 0;
            let liquidity = 5_000_000_000_000u128;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();

            // Create a limit order at tick -10 with a specific amount
            // The amount should be small enough that it can be fully consumed
            // while the swap still has remaining amount
            let limit_order_amount = 500_000u64;
            let mut tick_with_limit_order = build_tick(-10, 6_408_486_554, -6_408_486_554).take();
            tick_with_limit_order.orders_amount = limit_order_amount;

            // Build swap parameters with the limit order tick
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                1,
                sqrt_price_x64,
                liquidity,
                vec![
                    TickArrayInfo {
                        start_tick_index: 0,
                        ticks: vec![
                            build_tick(10, 0, 0).take(),
                            build_tick(20, 790_917_615_645, 0).take(),
                        ],
                    },
                    TickArrayInfo {
                        start_tick_index: -60,
                        ticks: vec![
                            build_tick(-20, 1_330_680_689, -1_330_680_689).take(),
                            tick_with_limit_order,
                        ],
                    },
                ],
            );

            // Get the initial unfilled amount before the swap
            let target_array_start =
                TickArrayState::get_array_start_index(-10, amm_config.tick_spacing);
            let mut limit_order_unfilled_amount_before = 0u64;
            for tick_array_ref in tick_array_states.iter() {
                let tick_array = tick_array_ref.borrow();
                if tick_array.start_tick_index == target_array_start {
                    // Find the tick at -10 by iterating through ticks
                    for tick_state in tick_array.ticks.iter() {
                        if tick_state.tick == -10 {
                            limit_order_unfilled_amount_before = tick_state
                                .orders_amount
                                .saturating_add(tick_state.part_filled_orders_remaining);
                            break;
                        }
                    }
                    break;
                }
            }
            assert_eq!(
                limit_order_unfilled_amount_before, limit_order_amount,
                "Initial unfilled amount should match limit order amount"
            );

            // Perform a swap that crosses the limit order tick but has enough remaining amount
            // to continue past it. The swap amount should be large enough that:
            // 1. It crosses tick -10 and executes the limit order
            // 2. It still has remaining amount after crossing (amount_specified_remaining != 0)
            // 3. The limit order should be fully consumed
            let swap_amount = 5_000_000_000u64; // Large enough to cross and have remaining
            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                swap_amount,
                tick_math::MIN_SQRT_PRICE_X64 + 1, // Price limit far below to allow continuation
                true,                              // zero_for_one
                true,                              // is_base_input
                oracle::block_timestamp_mock() as u32,
            );

            assert!(result.is_ok(), "Swap should succeed");
            let SwapInternalResult {
                amount_0, amount_1, ..
            } = result.unwrap();
            assert!(amount_0 > 0, "Swap should have produced output");
            assert!(amount_1 > 0, "Swap should have consumed input");

            // The swap should have crossed tick -10 (at least reached it)
            let final_tick = pool_state.borrow().tick_current;
            assert!(
                final_tick <= -10,
                "Swap should have at least reached tick -10, got {}",
                final_tick
            );

            // Get the unfilled amount after the swap
            let mut limit_order_unfilled_amount_after = 0u64;
            for tick_array_ref in tick_array_states.iter() {
                let tick_array = tick_array_ref.borrow();
                if tick_array.start_tick_index == target_array_start {
                    // Find the tick at -10 by iterating through ticks
                    for tick_state in tick_array.ticks.iter() {
                        if tick_state.tick == -10 {
                            limit_order_unfilled_amount_after = tick_state
                                .orders_amount
                                .saturating_add(tick_state.part_filled_orders_remaining);
                            break;
                        }
                    }
                    break;
                }
            }

            // Verify the invariant:
            // If the limit order was executed (unfilled amount changed) AND
            // the swap had remaining amount to continue past the tick,
            // then the limit order must be fully consumed (unfilled_amount_after == 0)
            if limit_order_unfilled_amount_after != limit_order_unfilled_amount_before {
                // The limit order was executed, so it must be fully consumed
                // because the swap had enough amount to continue past the tick
                assert_eq!(limit_order_unfilled_amount_after, 0);
            }
        }

        /// Tests for FIFO consumption when `part_filled_orders_remaining > 0`.
        ///
        /// `match_limit_order` consumes `part_filled_orders_remaining` first, then `orders_amount`.
        /// This module verifies:
        /// - Consuming only from `part_filled_orders_remaining` does NOT increment `order_phase`.
        /// - Consuming beyond `part_filled_orders_remaining` increments `order_phase` and moves the rest of
        ///   `orders_amount` into `part_filled_orders_remaining`.
        /// - When `part_filled_orders_remaining` is fully consumed and no other orders remain, liquidity crossing is allowed.
        mod partially_filled_orders_remaining_test {
            use super::*;

            fn build_pool_with_tick(
                tick_current: i32,
                tick_spacing: u16,
                sqrt_price_x64: u128,
                liquidity: u128,
                tick: TickState,
            ) -> (
                AmmConfig,
                RefCell<PoolState>,
                VecDeque<RefCell<TickArrayState>>,
                RefCell<ObservationState>,
            ) {
                let start_tick_index =
                    TickArrayState::get_array_start_index(tick.tick, tick_spacing);
                build_swap_param(
                    tick_current,
                    tick_spacing,
                    sqrt_price_x64,
                    liquidity,
                    vec![TickArrayInfo {
                        start_tick_index,
                        ticks: vec![tick],
                    }],
                )
            }

            #[test]
            fn test_consumes_part_remaining_first_no_epoch_increment() {
                let tick_spacing = 1;
                let tick_with_orders = 10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders - 1; // boundary assignment for one_for_zero
                let liquidity: u128 = 1_000;

                let mut tick_state = build_tick(tick_with_orders, 1, -100).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 1_000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Want to consume only 200 output, which is < part_remaining (500).
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(200, false, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false, // one_for_zero
                    true,  // is_base_input
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                // Still has limit orders => tick stays on `tick_next - 1` for one_for_zero.
                assert!(pool_state.borrow().tick_current == tick_current);
                assert!(pool_state.borrow().liquidity == liquidity);

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                // TickState is packed; copy fields to locals before asserting.
                let after_part_remaining = after.part_filled_orders_remaining;
                let after_orders_amount = after.orders_amount;
                let after_order_phase = after.order_phase;
                assert_eq!(after_part_remaining, 300);
                assert_eq!(after_orders_amount, 1_000);
                assert_eq!(
                    after_order_phase, 7,
                    "order_phase must not increment when only consuming part_remaining"
                );
            }

            #[test]
            fn test_consumes_beyond_part_remaining_increments_epoch_and_moves_orders() {
                let tick_spacing = 1;
                let tick_with_orders = 10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders - 1;
                let liquidity: u128 = 1_000;

                let mut tick_state = build_tick(tick_with_orders, 1, -100).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 1_000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Consume 700 output => 500 from part_remaining + 200 from orders_amount.
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(700, false, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    true,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let after_orders_amount = after.orders_amount;
                let after_part_remaining = after.part_filled_orders_remaining;
                let after_order_phase = after.order_phase;
                assert_eq!(after_orders_amount, 0);
                assert_eq!(
                    after_part_remaining, 800,
                    "remaining from orders_amount should move into part_remaining"
                );
                assert_eq!(
                    after_order_phase, 8,
                    "order_phase must increment when consuming from orders_amount"
                );
                assert!(
                    pool_state.borrow().tick_current == tick_current,
                    "still has limit orders => stay on tick_next-1"
                );
            }

            #[test]
            fn test_full_consumption_of_part_remaining_allows_cross() {
                let tick_spacing = 1;
                let tick_with_orders = 10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders - 1;
                let liquidity: u128 = 1_000;
                let liquidity_net: i128 = -100;

                let mut tick_state = build_tick(tick_with_orders, 1, liquidity_net).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 0;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Consume exactly all part_remaining.
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(500, false, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MAX_SQRT_PRICE_X64 - 1,
                    false,
                    true,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                // Orders cleared => can cross liquidity, and tick assignment becomes tick_next for one_for_zero.
                let tick_after = pool_state.borrow().tick_current;
                let liquidity_after = pool_state.borrow().liquidity;
                assert!(tick_after == tick_with_orders);
                assert!(
                    liquidity_after == liquidity_math::add_delta(liquidity, liquidity_net).unwrap()
                );

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                assert_eq!(after.limit_order_unfilled_amount().unwrap(), 0);
            }

            #[test]
            fn test_consumes_part_remaining_first_zero_for_one() {
                let tick_spacing = 1;
                let tick_with_orders = -10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders; // boundary assignment for zero_for_one + has_limit_orders
                let liquidity: u128 = 1_000;

                let mut tick_state = build_tick(tick_with_orders, 1, -100).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 1_000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Consume only 200 output (< part_remaining).
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(200, true, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true, // zero_for_one
                    true, // is_base_input
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                // Still has limit orders => tick stays on tick_next for zero_for_one.
                assert!(pool_state.borrow().tick_current == tick_current);
                assert!(pool_state.borrow().liquidity == liquidity);

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let after_part_remaining = after.part_filled_orders_remaining;
                let after_orders_amount = after.orders_amount;
                let after_order_phase = after.order_phase;
                assert_eq!(after_part_remaining, 300);
                assert_eq!(after_orders_amount, 1_000);
                assert_eq!(after_order_phase, 7);
            }

            #[test]
            fn test_consumes_beyond_part_remaining_zero_for_one_increments_epoch() {
                let tick_spacing = 1;
                let tick_with_orders = -10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders;
                let liquidity: u128 = 1_000;

                let mut tick_state = build_tick(tick_with_orders, 1, -100).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 1_000;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Consume 700 output => 500 from part_remaining + 200 from orders_amount.
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(700, true, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    true,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let after_orders_amount = after.orders_amount;
                let after_part_remaining = after.part_filled_orders_remaining;
                let after_order_phase = after.order_phase;

                assert_eq!(after_orders_amount, 0);

                assert_eq!(after_part_remaining, 800);
                assert_eq!(after_order_phase, 8);
                assert!(pool_state.borrow().tick_current == tick_current);
            }

            #[test]
            fn test_full_consumption_of_part_remaining_zero_for_one_allows_cross() {
                let tick_spacing = 1;
                let tick_with_orders = -10;
                let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_with_orders).unwrap();
                let tick_current = tick_with_orders;
                let liquidity: u128 = 1_000;
                let liquidity_net: i128 = -100;

                let mut tick_state = build_tick(tick_with_orders, 1, liquidity_net).take();
                tick_state.order_phase = 7;
                tick_state.part_filled_orders_remaining = 500;
                tick_state.orders_amount = 0;

                let (amm_config, pool_state, tick_array_states, observation_state) =
                    build_pool_with_tick(
                        tick_current,
                        tick_spacing,
                        sqrt_price_x64,
                        liquidity,
                        tick_state,
                    );

                // Consume exactly all part_remaining.
                let mut expected = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                let expected_match = expected
                    .match_limit_order(500, true, false, amm_config.trade_fee_rate, true)
                    .unwrap();
                let amount_specified = expected_match.amount_in + expected_match.amm_fee_amount;

                let _ = swap_internal(
                    &amm_config,
                    &mut pool_state.borrow_mut(),
                    &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                    &mut observation_state.borrow_mut(),
                    None,
                    amount_specified,
                    tick_math::MIN_SQRT_PRICE_X64 + 1,
                    true,
                    true,
                    oracle::block_timestamp_mock() as u32,
                )
                .unwrap();

                // Orders cleared => can cross liquidity, and for zero_for_one + !has_limit_orders => tick = tick_next - 1.
                let tick_after = pool_state.borrow().tick_current;
                let liquidity_after = pool_state.borrow().liquidity;
                assert!(tick_after == tick_with_orders - 1);
                assert!(
                    liquidity_after
                        == liquidity_math::add_delta(liquidity, liquidity_net.neg()).unwrap()
                );

                let after = get_tick_state(&tick_array_states, tick_with_orders).unwrap();
                assert_eq!(after.limit_order_unfilled_amount().unwrap(), 0);
            }
        }
    }

    #[cfg(test)]
    mod dynamic_fee_swap_test {
        use super::*;
        use crate::states::pool_fee::{
            tick_spacing_index_from_tick, MAX_FEE_RATE_NUMERATOR, VOLATILITY_ACCUMULATOR_SCALE,
        };
        use crate::states::tick_array_test::{build_tick, TickArrayInfo};

        /// Helper function to create a pool with dynamic fee enabled
        fn build_pool_with_dynamic_fee(
            tick_current: i32,
            tick_spacing: u16,
            sqrt_price_x64: u128,
            liquidity: u128,
            filter_period: u16,
            decay_period: u16,
            reduction_factor: u16,
            fee_control_factor: u32,
            max_volatility_accumulator: u32,
            timestamp: u64,
        ) -> RefCell<PoolState> {
            let pool_state = build_pool(tick_current, tick_spacing, sqrt_price_x64, liquidity);
            pool_state
                .borrow_mut()
                .initialize_dynamic_fee_info(
                    tick_current,
                    filter_period,
                    decay_period,
                    reduction_factor,
                    fee_control_factor,
                    max_volatility_accumulator,
                )
                .unwrap();

            // Initialize dynamic fee state
            {
                let mut pool = pool_state.borrow_mut();
                if pool.dynamic_fee_info != crate::states::pool_fee::DynamicFeeInfo::default() {
                    pool.dynamic_fee_info.last_update_timestamp = timestamp;
                    pool.dynamic_fee_info.volatility_reference = 0;
                    pool.dynamic_fee_info.volatility_accumulator = 0;
                }
            }

            pool_state
        }

        /// Test basic dynamic fee calculation
        #[test]
        fn test_basic_dynamic_fee_calculation() {
            let tick_current = 0;
            let tick_spacing = 60;
            let liquidity = 1_000_000_000_000u128;
            let base_fee_rate = 1000; // 0.1%
            let timestamp = 1000;

            // Setup dynamic fee with moderate parameters
            let filter_period = 60; // 1 minute
            let decay_period = 3600; // 1 hour
            let reduction_factor = 5000; // 0.5
            let fee_control_factor = 1000; // 0.01
            let max_volatility_accumulator = 100_000;

            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let pool_state = build_pool_with_dynamic_fee(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                filter_period,
                decay_period,
                reduction_factor,
                fee_control_factor,
                max_volatility_accumulator,
                timestamp,
            );

            // Create swap state and verify initial fee rate
            let swap_state = SwapState::new(
                &pool_state.borrow(),
                1_000_000u64,
                base_fee_rate,
                true, // zero_for_one
                timestamp,
            )
            .unwrap();

            // Initially, volatility_accumulator is 0, so dynamic fee should be 0
            let initial_fee_rate = swap_state.get_total_fee_rate().unwrap();
            assert_eq!(
                initial_fee_rate, base_fee_rate,
                "Initial fee should equal base fee"
            );

            // After crossing one tick spacing, volatility_accumulator should increase
            let mut swap_state_after_move = swap_state;
            swap_state_after_move.sqrt_price_x64 =
                tick_math::get_sqrt_price_at_tick(tick_spacing as i32).unwrap();
            swap_state_after_move.tick_spacing_index =
                tick_spacing_index_from_tick(tick_spacing as i32, tick_spacing);
            swap_state_after_move
                .update_volatility_accumulator()
                .unwrap();

            if let Some(dynamic_fee_info) = swap_state_after_move.dynamic_fee_info {
                // volatility_accumulator = volatility_reference + index_delta * VOLATILITY_ACCUMULATOR_SCALE
                // = 0 + 1 * 10000 = 10000
                let volatility_accumulator = dynamic_fee_info.volatility_accumulator;
                assert_eq!(volatility_accumulator, VOLATILITY_ACCUMULATOR_SCALE as u32);

                // Calculate expected dynamic fee rate
                // crossed = 10000 * 60 = 600000
                // squared = 600000^2 = 360000000000
                // denominator = 100000 * 10000 * 10000 = 10000000000000
                // fee_rate = ceil(1000 * 360000000000 / 10000000000000) = ceil(36) = 36
                let dynamic_fee_rate =
                    SwapState::compute_dynamic_fee_rate(&dynamic_fee_info, tick_spacing).unwrap();
                assert!(
                    dynamic_fee_rate > 0,
                    "Dynamic fee should be positive after movement"
                );
                assert!(
                    dynamic_fee_rate < 1000,
                    "Dynamic fee should be reasonable for small movement"
                );

                let total_fee_rate = base_fee_rate + dynamic_fee_rate;
                assert_eq!(
                    swap_state_after_move.get_total_fee_rate().unwrap(),
                    total_fee_rate
                );
            }
        }

        /// Test volatility accumulator update during swap
        #[test]
        fn test_volatility_accumulator_update() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let timestamp = 1000;

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000;
            let fee_control_factor = 1000;
            let max_volatility_accumulator = 100_000;

            let (_amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                vec![
                    TickArrayInfo {
                        start_tick_index: 0,
                        ticks: vec![
                            build_tick(60, liquidity / 2, 0).take(),
                            build_tick(120, liquidity / 2, 0).take(),
                        ],
                    },
                    TickArrayInfo {
                        start_tick_index: 3600,
                        ticks: vec![build_tick(3600, liquidity, 0).take()],
                    },
                ],
            );

            pool_state
                .borrow_mut()
                .initialize_dynamic_fee_info(
                    tick_current,
                    filter_period,
                    decay_period,
                    reduction_factor,
                    fee_control_factor,
                    max_volatility_accumulator,
                )
                .unwrap();

            {
                let mut pool = pool_state.borrow_mut();
                if pool.dynamic_fee_info != crate::states::pool_fee::DynamicFeeInfo::default() {
                    pool.dynamic_fee_info.last_update_timestamp = timestamp;
                    pool.dynamic_fee_info.volatility_reference = 0;
                    pool.dynamic_fee_info.volatility_accumulator = 0;
                }
            }

            // Perform a swap that crosses multiple tick spacings
            let swap_amount = 10_000_000_000u64; // Large swap to cross ticks
            let _ = swap_internal(
                &_amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                swap_amount,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                false, // zero_for_one = false (swap token1 -> token0, price goes up)
                true,  // is_base_input
                timestamp as u32,
            )
            .unwrap();

            // Verify volatility accumulator was updated
            {
                let pool = pool_state.borrow();
                if let Some(dynamic_fee_info) = pool.get_dynamic_fee_info() {
                    assert!(
                        dynamic_fee_info.volatility_accumulator > 0,
                        "Volatility accumulator should increase after swap"
                    );
                    assert!(
                        dynamic_fee_info.volatility_accumulator <= max_volatility_accumulator,
                        "Volatility accumulator should not exceed max"
                    );
                }
            }
        }

        /// Table-driven test for `DynamicFeeInfo::update_reference` time windows.
        /// Covers: filter_period (no update), decay_period (decay), beyond decay_period (reset).
        #[test]
        fn test_update_reference_time_windows() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let timestamp = 1000u64;

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000; // 0.5
            let fee_control_factor = 1000;
            let max_volatility_accumulator = 100_000;

            // (delta_ts, expected_vol_ref, expect_tick_ref_updated, expect_ts_updated)
            let cases = [
                (30u64, 20_000u32, false, false), // within filter_period
                (1800u64, 25_000u32, true, true), // within decay_period
                (7200u64, 0u32, true, true),      // beyond decay_period
            ];

            for (delta_ts, expected_vol_ref, expect_tick_ref_updated, expect_ts_updated) in cases {
                let pool_state = build_pool_with_dynamic_fee(
                    tick_current,
                    tick_spacing,
                    sqrt_price_x64,
                    liquidity,
                    filter_period,
                    decay_period,
                    reduction_factor,
                    fee_control_factor,
                    max_volatility_accumulator,
                    timestamp,
                );

                let new_tick_spacing_index = 5;
                let new_timestamp = timestamp + delta_ts;

                let (old_reference, old_tick_reference, old_timestamp) = {
                    let mut pool = pool_state.borrow_mut();
                    // Set initial volatility for the test.
                    pool.dynamic_fee_info.volatility_accumulator = 50_000;
                    pool.dynamic_fee_info.volatility_reference = 20_000;
                    (
                        pool.dynamic_fee_info.volatility_reference,
                        pool.dynamic_fee_info.tick_spacing_index_reference,
                        pool.dynamic_fee_info.last_update_timestamp,
                    )
                };

                {
                    let mut pool = pool_state.borrow_mut();
                    pool.dynamic_fee_info
                        .update_reference(new_tick_spacing_index, new_timestamp)
                        .unwrap();

                    let volatility_ref = pool.dynamic_fee_info.volatility_reference;
                    let tick_spacing_ref = pool.dynamic_fee_info.tick_spacing_index_reference;
                    let last_update_ts = pool.dynamic_fee_info.last_update_timestamp;

                    assert_eq!(volatility_ref, expected_vol_ref);

                    if expect_tick_ref_updated {
                        assert_eq!(tick_spacing_ref, new_tick_spacing_index);
                    } else {
                        assert_eq!(tick_spacing_ref, old_tick_reference);
                    }

                    if expect_ts_updated {
                        assert_eq!(last_update_ts, new_timestamp);
                    } else {
                        assert_eq!(last_update_ts, old_timestamp);
                    }

                    // When no update is expected, reference should remain unchanged.
                    if !expect_ts_updated {
                        assert_eq!(volatility_ref, old_reference);
                    }
                }
            }
        }

        /// Test fee rate cap at MAX_FEE_RATE_NUMERATOR
        #[test]
        fn test_fee_rate_cap() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let base_fee_rate = 50_000; // 5%
            let timestamp = 1000;

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000;
            let fee_control_factor = 50_000; // High control factor to generate large dynamic fee
            let max_volatility_accumulator = 100_000;

            let pool_state = build_pool_with_dynamic_fee(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                filter_period,
                decay_period,
                reduction_factor,
                fee_control_factor,
                max_volatility_accumulator,
                timestamp,
            );

            // Set very high volatility accumulator
            {
                let mut pool = pool_state.borrow_mut();
                if pool.dynamic_fee_info != crate::states::pool_fee::DynamicFeeInfo::default() {
                    pool.dynamic_fee_info.volatility_accumulator = max_volatility_accumulator;
                }
            }

            let swap_state = SwapState::new(
                &pool_state.borrow(),
                1_000_000u64,
                base_fee_rate,
                true,
                timestamp,
            )
            .unwrap();

            // Update volatility accumulator to max
            let mut swap_state_with_max_vol = swap_state;
            if let Some(ref mut dynamic_fee_info) = swap_state_with_max_vol.dynamic_fee_info {
                dynamic_fee_info.volatility_accumulator = max_volatility_accumulator;
            }

            let total_fee_rate = swap_state_with_max_vol.get_total_fee_rate().unwrap();

            // Total fee rate should be capped at MAX_FEE_RATE_NUMERATOR
            assert!(
                total_fee_rate <= MAX_FEE_RATE_NUMERATOR,
                "Total fee rate should not exceed MAX_FEE_RATE_NUMERATOR"
            );
            assert_eq!(
                total_fee_rate,
                MAX_FEE_RATE_NUMERATOR,
                "Total fee rate should be capped at MAX_FEE_RATE_NUMERATOR when base + dynamic exceeds it"
            );
        }

        /// Test volatility accumulator accumulation across multiple swaps
        #[test]
        fn test_volatility_accumulation_multiple_swaps() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let timestamp = 1000;

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000;
            let fee_control_factor = 1000;
            let max_volatility_accumulator = 100_000;

            let (_amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                vec![
                    TickArrayInfo {
                        start_tick_index: 0,
                        ticks: vec![
                            build_tick(60, liquidity / 3, 0).take(),
                            build_tick(120, liquidity / 3, 0).take(),
                            build_tick(180, liquidity / 3, 0).take(),
                        ],
                    },
                    TickArrayInfo {
                        start_tick_index: 3600,
                        ticks: vec![build_tick(3600, liquidity, 0).take()],
                    },
                ],
            );

            pool_state
                .borrow_mut()
                .initialize_dynamic_fee_info(
                    tick_current,
                    filter_period,
                    decay_period,
                    reduction_factor,
                    fee_control_factor,
                    max_volatility_accumulator,
                )
                .unwrap();

            {
                let mut pool = pool_state.borrow_mut();
                if pool.dynamic_fee_info != crate::states::pool_fee::DynamicFeeInfo::default() {
                    pool.dynamic_fee_info.last_update_timestamp = timestamp;
                    pool.dynamic_fee_info.volatility_reference = 0;
                    pool.dynamic_fee_info.volatility_accumulator = 0;
                }
            }

            // Perform first swap
            let swap_amount1 = 5_000_000_000u64;
            swap_internal(
                &_amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                swap_amount1,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                false, // zero_for_one = false (price up)
                true,
                timestamp as u32,
            )
            .unwrap();

            let volatility_after_first = pool_state
                .borrow()
                .get_dynamic_fee_info()
                .map(|info| info.volatility_accumulator)
                .unwrap_or(0);

            // Perform second swap in same direction
            let swap_amount2 = 5_000_000_000u64;
            swap_internal(
                &_amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                None,
                swap_amount2,
                tick_math::MAX_SQRT_PRICE_X64 - 1,
                false, // zero_for_one = false (price up)
                true,
                (timestamp + 10) as u32,
            )
            .unwrap();

            let volatility_after_second = pool_state
                .borrow()
                .get_dynamic_fee_info()
                .map(|info| info.volatility_accumulator)
                .unwrap_or(0);

            // Volatility should accumulate
            assert!(
                volatility_after_second >= volatility_after_first,
                "Volatility accumulator should increase with consecutive swaps"
            );
        }

        /// Test bidirectional swaps and volatility accumulation

        /// Test that dynamic fee is disabled when not initialized
        #[test]
        fn test_dynamic_fee_disabled() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let base_fee_rate = 1000;
            let timestamp = 1000;

            let (_amm_config, pool_state, _, _) = build_swap_param(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: 0,
                    ticks: vec![build_tick(60, liquidity, 0).take()],
                }],
            );

            // Pool without dynamic fee
            let swap_state = SwapState::new(
                &pool_state.borrow(),
                1_000_000u64,
                base_fee_rate,
                true,
                timestamp,
            )
            .unwrap();

            // Should only use base fee
            assert_eq!(
                swap_state.get_total_fee_rate().unwrap(),
                base_fee_rate,
                "Fee rate should equal base fee when dynamic fee is disabled"
            );
            assert!(
                swap_state.dynamic_fee_info.is_none(),
                "Dynamic fee info should be None when not initialized"
            );
        }

        /// Test volatility accumulator capping at max_volatility_accumulator
        #[test]
        fn test_volatility_accumulator_cap() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let timestamp = 1000;

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000;
            let fee_control_factor = 1000;
            let max_volatility_accumulator = 50_000;

            let pool_state = build_pool_with_dynamic_fee(
                tick_current,
                tick_spacing,
                sqrt_price_x64,
                liquidity,
                filter_period,
                decay_period,
                reduction_factor,
                fee_control_factor,
                max_volatility_accumulator,
                timestamp,
            );

            // Try to set volatility accumulator beyond max
            {
                let mut pool = pool_state.borrow_mut();
                if pool.dynamic_fee_info != crate::states::pool_fee::DynamicFeeInfo::default() {
                    // Set reference to a far away tick spacing index
                    pool.dynamic_fee_info.tick_spacing_index_reference = 0;
                    // Try to update with a very large index delta
                    pool.dynamic_fee_info.volatility_reference = 0;
                    // This should cap at max_volatility_accumulator
                    pool.dynamic_fee_info
                        .update_volatility_accumulator(100) // Very large tick spacing index
                        .unwrap();

                    let volatility_acc = pool.dynamic_fee_info.volatility_accumulator;
                    assert!(
                        volatility_acc <= max_volatility_accumulator,
                        "Volatility accumulator should be capped at max_volatility_accumulator"
                    );
                    assert_eq!(
                        volatility_acc, max_volatility_accumulator,
                        "Volatility accumulator should equal max when delta exceeds it"
                    );
                }
            }
        }

        /// Cross many tick-groups in a single swap, with *no initialized ticks in between*.
        ///
        /// This specifically guards the "per-group update" design: if the swap loop does not
        /// bound each step by tick-group boundaries, then with no initialized ticks the loop
        /// can become a single large step and the dynamic fee would remain at its initial value
        /// for the whole swap (under-charging fees).
        #[test]
        fn test_dynamic_fee_applies_per_group_without_initialized_ticks() {
            let tick_current = 0;
            let tick_spacing = 60;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let liquidity = 1_000_000_000_000u128;
            let timestamp = 1000u64;

            // Swap direction: token1 -> token0, price goes up (ticks increase).
            let zero_for_one = false;

            // End exactly at a known tick-group boundary far away from the reference.
            let target_tick = 6000;
            let sqrt_price_limit_x64 = tick_math::get_sqrt_price_at_tick(target_tick).unwrap();

            // Provide the tick arrays covering the entire path, but with *no initialized ticks*.
            // (We build two identical Vecs to avoid requiring `TickArrayInfo: Clone`.)
            let build_empty_tick_arrays = || {
                // Ensure each tick array has at least one initialized tick, otherwise swap will
                // fail at `first_initialized_tick` with `InvalidTickArray`.
                //
                // We put the initialized tick at the far end of the second array (beyond the
                // price limit), so there are effectively no initialized ticks between the current
                // price (tick 0) and the swap limit (tick 6000).
                let far_initialized_tick_in_second_array = build_tick(7140, 1, 0).take();
                vec![
                    TickArrayInfo {
                        start_tick_index: 0,
                        ticks: vec![],
                    },
                    TickArrayInfo {
                        start_tick_index: 3600,
                        ticks: vec![far_initialized_tick_in_second_array],
                    },
                ]
            };

            // Baseline: dynamic fee disabled, but fee is collected from output (token0) so that
            // both swaps have the same net input and thus reach the same price limit.
            let (amm_config_base, pool_state_base, tick_array_states_base, observation_state_base) =
                build_swap_param(
                    tick_current,
                    tick_spacing,
                    sqrt_price_x64,
                    liquidity,
                    build_empty_tick_arrays(),
                );
            pool_state_base.borrow_mut().set_fee_on(1).unwrap(); // Token0Only => fee from output for zero_for_one=false

            // Dynamic fee enabled (same initial state / tick arrays).
            let (amm_config_dyn, pool_state_dyn, tick_array_states_dyn, observation_state_dyn) =
                build_swap_param(
                    tick_current,
                    tick_spacing,
                    sqrt_price_x64,
                    liquidity,
                    build_empty_tick_arrays(),
                );
            pool_state_dyn.borrow_mut().set_fee_on(1).unwrap(); // Token0Only

            let filter_period = 60;
            let decay_period = 3600;
            let reduction_factor = 5000;
            let fee_control_factor = 10; // small control to avoid hitting MAX_FEE_RATE_NUMERATOR
            let max_volatility_accumulator = 2_000_000;
            pool_state_dyn
                .borrow_mut()
                .initialize_dynamic_fee_info(
                    tick_current,
                    filter_period,
                    decay_period,
                    reduction_factor,
                    fee_control_factor,
                    max_volatility_accumulator,
                )
                .unwrap();
            {
                let mut pool = pool_state_dyn.borrow_mut();
                pool.dynamic_fee_info.last_update_timestamp = timestamp;
                pool.dynamic_fee_info.volatility_reference = 0;
                pool.dynamic_fee_info.volatility_accumulator = 0;
            }

            // Use a large net input so both swaps hit `sqrt_price_limit_x64` and stop.
            let amount_in_net = 10_000_000_000_000u64;

            let fees_token0_base_before = pool_state_base.borrow().fee_growth_global_0_x64;
            swap_internal(
                &amm_config_base,
                &mut pool_state_base.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states_base).borrow_mut(),
                &mut observation_state_base.borrow_mut(),
                None,
                amount_in_net,
                sqrt_price_limit_x64,
                zero_for_one,
                true, // exact input
                timestamp as u32,
            )
            .unwrap();
            let fees_token0_base_after = pool_state_base.borrow().fee_growth_global_0_x64;
            let fees_token0_base = fees_token0_base_after - fees_token0_base_before;

            let fees_token0_dyn_before = pool_state_dyn.borrow().fee_growth_global_0_x64;
            swap_internal(
                &amm_config_dyn,
                &mut pool_state_dyn.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states_dyn).borrow_mut(),
                &mut observation_state_dyn.borrow_mut(),
                None,
                amount_in_net,
                sqrt_price_limit_x64,
                zero_for_one,
                true, // exact input
                timestamp as u32,
            )
            .unwrap();
            let fees_token0_dyn_after = pool_state_dyn.borrow().fee_growth_global_0_x64;
            let fees_token0_dyn = fees_token0_dyn_after - fees_token0_dyn_before;

            // Both swaps should reach the same price limit because fee is on output and net input is equal.
            let sqrt_price_base = { pool_state_base.borrow().sqrt_price_x64 };
            let sqrt_price_dyn = { pool_state_dyn.borrow().sqrt_price_x64 };
            assert_eq!(sqrt_price_base, sqrt_price_limit_x64);
            assert_eq!(sqrt_price_dyn, sqrt_price_limit_x64);

            // With per-group updates, dynamic-fee swap should charge more than base-only swap.
            assert!(
                fees_token0_dyn > fees_token0_base,
                "Dynamic-fee swap should record more output fees when crossing many groups without initialized ticks (base={}, dyn={})",
                fees_token0_base,
                fees_token0_dyn
            );

            // And the final persisted accumulator should match the end group's distance from reference.
            let expected_index = tick_spacing_index_from_tick(target_tick, tick_spacing);
            let expected_volatility = (u64::from(expected_index.unsigned_abs())
                * u64::from(VOLATILITY_ACCUMULATOR_SCALE))
            .min(u64::from(max_volatility_accumulator))
                as u32;

            let volatility_accumulator = pool_state_dyn
                .borrow()
                .get_dynamic_fee_info()
                .map(|info| info.volatility_accumulator)
                .unwrap_or(0);
            assert_eq!(volatility_accumulator, expected_volatility);
        }
    }
    #[cfg(test)]
    mod fee_collect_mode_test {
        use super::*;

        /// Test fee collection mode comparison when token0 is the output token
        #[test]
        fn compare_fee_collection_when_token0_is_output() {
            // Setup: zero_for_one=false means swapping token1 -> token0
            // fee_on=1 (Token0Only) with zero_for_one=false -> is_fee_on_input=false (fee from output token0)
            let tick_current = -32470;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3638127228312488926;
            let swap_amount = 887470480u64;
            let zero_for_one = false;

            // Test 1: fee_from_input = true (default, fee_on = 0)
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap(); // FromInput (default)
            assert!(pool_state1.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before1 = pool_state1.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_1: amount_1_fee_from_input,
                ..
            } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                swap_amount,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after1 = pool_state1.borrow().fee_growth_global_1_x64;
            let fees_recorded1 = if pool_state1.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after1 - total_fees_token0_before1
            } else {
                total_fees_token1_after1 - total_fees_token1_before1
            };

            // Test 2: fee_from_input = false (fee_on = 1, Token0Only)
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(1).unwrap(); // Token0Only
            assert!(!pool_state2.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before2 = pool_state2.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_1: amount_1_fee_from_output,
                ..
            } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                swap_amount,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after2 = pool_state2.borrow().fee_growth_global_1_x64;
            let fees_recorded2 = if pool_state2.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after2 - total_fees_token0_before2
            } else {
                total_fees_token1_after2 - total_fees_token1_before2
            };

            // Verification 1: Input amount behavior
            // swap_internal returns (amount_0, amount_1) where:
            // - When zero_for_one=false: (amount_out_token0, amount_in_token1)
            // - When fee_from_input: amount_in_token1 = net input (amount_in without fee)
            // - When fee_from_output: amount_in_token1 = net input (same as swap_amount for exact input)
            //
            // For exact input swap with fee_from_input:
            // - amount_specified is gross input (includes fee)
            // - Returns net input (amount_in)
            // For exact input swap with fee_from_output:
            // - amount_specified is net input
            // - Returns net input (same as amount_specified)
            assert_eq!(
                amount_1_fee_from_output, swap_amount,
                "With fee_from_output, user pays exactly swap_amount (net input)"
            );
            assert!(
                amount_1_fee_from_input <= swap_amount,
                "With fee_from_input (gross specified), returned net input should be <= amount_specified"
            );

            // Verification 2: Fee amounts should be positive and reasonable
            assert!(
                fees_recorded1 > 0,
                "Fees should be recorded when fee is from input, recorded amount: {}",
                fees_recorded1
            );
            assert!(
                fees_recorded2 > 0,
                "Fees should be recorded when fee is from output, recorded amount: {}",
                fees_recorded2
            );

            // Verification 3: Fee recording location
            // fee_on=0 (FromInput) with zero_for_one=false:
            // - is_fee_on_token0(false) = false, so fees recorded to token1 (input token)
            assert!(
                total_fees_token1_after1 > total_fees_token1_before1,
                "fee_on=0 should record fees to token1 (input token) when zero_for_one=false"
            );
            assert_eq!(
                total_fees_token0_after1, total_fees_token0_before1,
                "fee_on=0 should not record fees to token0 when zero_for_one=false"
            );

            // fee_on=1 (Token0Only) with zero_for_one=false:
            // - is_fee_on_token0(false) = true, so fees always recorded to token0 (output token)
            assert!(
                total_fees_token0_after2 > total_fees_token0_before2,
                "fee_on=1 (Token0Only) should always record fees to token0"
            );
            assert_eq!(
                total_fees_token1_after2, total_fees_token1_before2,
                "fee_on=1 (Token0Only) should not record fees to token1"
            );

            // Verification 4: Pool state consistency
            // Both swaps should have consumed liquidity and updated price
            assert!(
                pool_state1.borrow().sqrt_price_x64 != sqrt_price_x64
                    || pool_state2.borrow().sqrt_price_x64 != sqrt_price_x64,
                "Pool price should have changed after swap"
            );
        }

        /// Test fee collection mode comparison when token1 is the output token
        #[test]
        fn compare_fee_collection_when_token1_is_output() {
            // Setup: zero_for_one=true means swapping token0 -> token1
            // fee_on=2 (Token1Only) with zero_for_one=true -> is_fee_on_input=false (fee from output token1)
            let tick_current = -32395;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3651942632306380802;
            let swap_amount = 12188240002u64;
            let zero_for_one = true;

            // Test 1: fee_from_input = true (default, fee_on = 0)
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap(); // FromInput (default)
            assert!(pool_state1.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before1 = pool_state1.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: _amount_0_fee_from_input,
                ..
            } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                swap_amount,
                3049500711113990606,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after1 = pool_state1.borrow().fee_growth_global_1_x64;
            let fees_recorded1 = if pool_state1.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after1 - total_fees_token0_before1
            } else {
                total_fees_token1_after1 - total_fees_token1_before1
            };

            // Test 2: fee_from_input = false (fee_on = 2, Token1Only)
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(2).unwrap(); // Token1Only
            assert!(!pool_state2.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before2 = pool_state2.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: amount_0_fee_from_output,
                ..
            } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                swap_amount,
                3049500711113990606,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after2 = pool_state2.borrow().fee_growth_global_1_x64;
            let fees_recorded2 = if pool_state2.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after2 - total_fees_token0_before2
            } else {
                total_fees_token1_after2 - total_fees_token1_before2
            };

            // Verification 1: Input amount behavior
            // swap_internal returns (amount_0, amount_1) where:
            // - When zero_for_one=true: (amount_in_token0, amount_out_token1)
            // - When fee_from_input: amount_in_token0 = net input (amount_in without fee)
            // - When fee_from_output: amount_in_token0 = net input (same as swap_amount for exact input)
            assert_eq!(
                amount_0_fee_from_output, swap_amount,
                "With fee_from_output, user pays exactly swap_amount (net input)"
            );
            assert!(
                _amount_0_fee_from_input <= swap_amount,
                "With fee_from_input (gross specified), returned net input should be <= amount_specified"
            );

            // Verification 2: Fee amounts should be positive and reasonable
            assert!(
                fees_recorded1 > 0,
                "Fees should be recorded when fee is from input, recorded amount: {}",
                fees_recorded1
            );
            assert!(
                fees_recorded2 > 0,
                "Fees should be recorded when fee is from output, recorded amount: {}",
                fees_recorded2
            );

            // Verification 3: Fee recording location
            // fee_on=0 (FromInput) with zero_for_one=true:
            // - is_fee_on_token0(true) = true, so fees recorded to token0 (input token)
            assert!(
                total_fees_token0_after1 > total_fees_token0_before1,
                "fee_on=0 should record fees to token0 (input token) when zero_for_one=true"
            );
            assert_eq!(
                total_fees_token1_after1, total_fees_token1_before1,
                "fee_on=0 should not record fees to token1 when zero_for_one=true"
            );

            // fee_on=2 (Token1Only) with zero_for_one=true:
            // - is_fee_on_token0(true) = false, so fees always recorded to token1 (output token)
            assert!(
                total_fees_token1_after2 > total_fees_token1_before2,
                "fee_on=2 (Token1Only) should always record fees to token1"
            );
            assert_eq!(
                total_fees_token0_after2, total_fees_token0_before2,
                "fee_on=2 (Token1Only) should not record fees to token0"
            );

            // Verification 4: Pool state consistency
            // Both swaps should have consumed liquidity and updated price
            assert!(
                pool_state1.borrow().sqrt_price_x64 != sqrt_price_x64
                    || pool_state2.borrow().sqrt_price_x64 != sqrt_price_x64,
                "Pool price should have changed after swap"
            );
        }

        /// Test Exact Output Swap with fee collection modes when token0 is the output token
        #[test]
        fn compare_fee_collection_exact_output_token0() {
            // Setup: zero_for_one=false means swapping token1 -> token0
            let tick_current = -32470;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3638127228312488926;
            let desired_output = 1000000u64; // desired output amount
            let zero_for_one = false;

            // Test 1: fee_from_input = true (default, fee_on = 0)
            // amount_specified is gross output (user wants this much output)
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap(); // FromInput
            assert!(pool_state1.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before1 = pool_state1.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: amount_0_fee_from_input,
                amount_1: amount_1_fee_from_input,
                ..
            } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                desired_output,
                5882283448660210779,
                zero_for_one,
                false, // is_base_input = false (Exact Output Swap)
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after1 = pool_state1.borrow().fee_growth_global_1_x64;
            let fees_recorded1 = if pool_state1.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after1 - total_fees_token0_before1
            } else {
                total_fees_token1_after1 - total_fees_token1_before1
            };

            // Test 2: fee_from_output = false (fee_on = 1, Token0Only)
            // amount_specified is net output (user wants this much output after fee)
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(1).unwrap(); // Token0Only
            assert!(!pool_state2.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before2 = pool_state2.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: amount_0_fee_from_output,
                amount_1: amount_1_fee_from_output,
                ..
            } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                desired_output,
                5882283448660210779,
                zero_for_one,
                false, // is_base_input = false (Exact Output Swap)
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after2 = pool_state2.borrow().fee_growth_global_1_x64;
            let fees_recorded2 = if pool_state2.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after2 - total_fees_token0_before2
            } else {
                total_fees_token1_after2 - total_fees_token1_before2
            };

            // Verification 1: Output amount behavior
            // swap_internal returns (amount_0, amount_1) where:
            // - When zero_for_one=false: (amount_out_token0, amount_in_token1)
            // - With fee_from_input: amount_0 should equal desired_output (gross output)
            // - With fee_from_output: amount_0 should equal desired_output (net output)
            assert_eq!(
                amount_0_fee_from_input, desired_output,
                "With fee_from_input, user receives gross output equal to desired_output"
            );
            assert_eq!(
                amount_0_fee_from_output, desired_output,
                "With fee_from_output, user receives net output equal to desired_output"
            );

            // Verification 2: Input amounts should be close (they can differ due to rounding/slippage).
            let input_diff = if amount_1_fee_from_output > amount_1_fee_from_input {
                amount_1_fee_from_output - amount_1_fee_from_input
            } else {
                amount_1_fee_from_input - amount_1_fee_from_output
            };
            let max_diff = (amount_1_fee_from_output.max(amount_1_fee_from_input) / 100).max(1000);
            assert!(
                input_diff <= max_diff,
                "Exact-output input amounts should be close: fee_from_input={}, fee_from_output={}, diff={}, max_diff={}",
                amount_1_fee_from_input,
                amount_1_fee_from_output,
                input_diff,
                max_diff
            );

            // Verification 3: Fee amounts should be positive
            assert!(
                fees_recorded1 > 0,
                "Fees should be recorded when fee is from input"
            );
            assert!(
                fees_recorded2 > 0,
                "Fees should be recorded when fee is from output"
            );

            // Verification 4: Fee recording location
            assert!(
                total_fees_token1_after1 > total_fees_token1_before1,
                "fee_on=0 should record fees to token1 (input token)"
            );
            assert_eq!(
                total_fees_token0_after1, total_fees_token0_before1,
                "fee_on=0 should not record fees to token0"
            );
            assert!(
                total_fees_token0_after2 > total_fees_token0_before2,
                "fee_on=1 (Token0Only) should record fees to token0"
            );
            assert_eq!(
                total_fees_token1_after2, total_fees_token1_before2,
                "fee_on=1 (Token0Only) should not record fees to token1"
            );
        }

        /// Test Exact Output Swap with fee collection modes when token1 is the output token
        #[test]
        fn compare_fee_collection_exact_output_token1() {
            // Setup: zero_for_one=true means swapping token0 -> token1
            let tick_current = -32395;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3651942632306380802;
            let desired_output = 1_000_000u64; // desired output amount (token1)
            let zero_for_one = true;

            // Test 1: fee_from_input = true (default, fee_on = 0)
            // amount_specified is desired gross output (token1). Since fee is from input, output is unaffected.
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap(); // FromInput
            assert!(pool_state1.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before1 = pool_state1.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: amount_0_fee_from_input,
                amount_1: amount_1_fee_from_input,
                ..
            } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                desired_output,
                3049500711113990606,
                zero_for_one,
                false, // exact output
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after1 = pool_state1.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after1 = pool_state1.borrow().fee_growth_global_1_x64;
            let fees_recorded1 = if pool_state1.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after1 - total_fees_token0_before1
            } else {
                total_fees_token1_after1 - total_fees_token1_before1
            };

            // Test 2: fee_from_output (fee_on = 2, Token1Only) -> fee from output token1
            // amount_specified is desired net output token1 (after fee deduction).
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(2).unwrap(); // Token1Only
            assert!(!pool_state2.borrow().is_fee_on_input(zero_for_one));
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_before2 = pool_state2.borrow().fee_growth_global_1_x64;
            let SwapInternalResult {
                amount_0: amount_0_fee_from_output,
                amount_1: amount_1_fee_from_output,
                ..
            } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                desired_output,
                3049500711113990606,
                zero_for_one,
                false, // exact output
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            let total_fees_token1_after2 = pool_state2.borrow().fee_growth_global_1_x64;
            let fees_recorded2 = if pool_state2.borrow().is_fee_on_token0(zero_for_one) {
                total_fees_token0_after2 - total_fees_token0_before2
            } else {
                total_fees_token1_after2 - total_fees_token1_before2
            };

            // Outputs should match desired_output for both modes (gross vs net semantics are aligned for user output).
            assert_eq!(amount_1_fee_from_input, desired_output);
            assert_eq!(amount_1_fee_from_output, desired_output);
            // Both should require non-zero input.
            assert!(amount_0_fee_from_input > 0);
            assert!(amount_0_fee_from_output > 0);

            // Fee amounts should be positive.
            assert!(fees_recorded1 > 0);
            assert!(fees_recorded2 > 0);

            // Fee recording location:
            // fee_on=0 with zero_for_one=true => fee recorded to token0 (input token)
            assert!(total_fees_token0_after1 > total_fees_token0_before1);
            assert_eq!(total_fees_token1_after1, total_fees_token1_before1);
            // fee_on=2 always records to token1 (output token)
            assert!(total_fees_token1_after2 > total_fees_token1_before2);
            assert_eq!(total_fees_token0_after2, total_fees_token0_before2);
        }

        /// Verify fee calculation accuracy for both fee collection modes
        #[test]
        fn verify_fee_calculation_accuracy() {
            let tick_current = -32470;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3638127228312488926;
            let swap_amount = 887470480u64;
            let zero_for_one = false;

            // Test 1: fee_from_input (fee_on=0)
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap();
            let total_fees_token1_before1 = pool_state1.borrow().fee_growth_global_1_x64;
            let SwapInternalResult { .. } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                swap_amount,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token1_after1 = pool_state1.borrow().fee_growth_global_1_x64;
            assert!(
                total_fees_token1_after1 > total_fees_token1_before1,
                "Fee growth should increase for fee_from_input"
            );

            // Test 2: fee_from_output (fee_on=1)
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(1).unwrap();
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let SwapInternalResult { .. } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                swap_amount,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            let total_fees_token0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            assert!(
                total_fees_token0_after2 > total_fees_token0_before2,
                "Fee growth should increase for fee_from_output"
            );
        }

        /// Compare behavior with same gross input for both fee collection modes
        ///
        /// When using the same *net input into the swap math* (i.e., the amount that actually
        /// moves the price):
        /// - Both modes should produce the same gross output (up to rounding)
        /// - But net output differs: fee_from_output has less net output (fee deducted)
        #[test]
        fn compare_with_same_net_input() {
            let tick_current = -32470;
            let liquidity = 5124165121219;
            let sqrt_price_x64 = 3638127228312488926;
            let zero_for_one = false;

            // First, do a swap with fee_from_input to get gross input
            let (amm_config1, pool_state1, tick_array_states1, observation_state1) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state1.borrow_mut().set_fee_on(0).unwrap();
            let swap_amount1 = 887470480u64;
            let SwapInternalResult {
                amount_0: amount_0_1,
                amount_1: amount_1_1,
                ..
            } = swap_internal(
                &amm_config1,
                &mut pool_state1.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states1).borrow_mut(),
                &mut observation_state1.borrow_mut(),
                None,
                swap_amount1,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();

            // Gross output for fee_from_input (fee deducted from input, output unaffected).
            let gross_output1 = amount_0_1; // For fee_from_input, output is gross

            // Now do swap with fee_from_output using net input = amount_1_1
            let (amm_config2, pool_state2, tick_array_states2, observation_state2) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                            ],
                        },
                    ],
                );
            pool_state2.borrow_mut().set_fee_on(1).unwrap();
            let net_input2 = amount_1_1; // Use same net input
            let total_fees_token0_before2 = pool_state2.borrow().fee_growth_global_0_x64;
            let SwapInternalResult {
                amount_0: amount_0_2,
                ..
            } = swap_internal(
                &amm_config2,
                &mut pool_state2.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states2).borrow_mut(),
                &mut observation_state2.borrow_mut(),
                None,
                net_input2,
                5882283448660210779,
                zero_for_one,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();

            let fee_growth_0_after2 = pool_state2.borrow().fee_growth_global_0_x64;
            assert!(
                fee_growth_0_after2 > total_fees_token0_before2,
                "fee_from_output should record token0 fee"
            );

            // Estimate fee from fee rate: fee ≈ net_output * fee_rate / (FEE_RATE_DENOMINATOR - fee_rate)
            let fee_rate = amm_config2.trade_fee_rate as u64;
            let fee2_estimate = amount_0_2
                .mul_div_ceil(fee_rate, FEE_RATE_DENOMINATOR_VALUE as u64 - fee_rate)
                .unwrap();
            let gross_output2 = amount_0_2 + fee2_estimate; // net output + fee ≈ gross output

            // Verification: Gross outputs should be similar (with rounding differences)
            // When using same net input, gross outputs should be close
            let output_diff = if gross_output1 > gross_output2 {
                gross_output1 - gross_output2
            } else {
                gross_output2 - gross_output1
            };
            // Allow reasonable difference due to rounding (within 1% or 10000 units, whichever is larger)
            let max_diff = (gross_output1.max(gross_output2) / 100).max(10000);
            assert!(
                output_diff <= max_diff,
                "Gross outputs should be similar: fee_from_input={}, fee_from_output={}, diff={}, max_diff={}",
                gross_output1,
                gross_output2,
                output_diff,
                max_diff
            );

            // Verification: Net output with fee_from_output should be less
            assert!(
                amount_0_2 < amount_0_1,
                "Net output with fee_from_output ({}) should be less than gross output with fee_from_input ({})",
                amount_0_2,
                amount_0_1
            );
        }
    }
}
