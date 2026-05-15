use crate::error::ErrorCode;
use crate::libraries::U128;
use crate::libraries::{fixed_point_64, full_math::MulDiv, liquidity_math, tick_math};
use crate::pool::{RewardInfo, REWARD_NUM};
use crate::states::config::FEE_RATE_DENOMINATOR_VALUE;
use crate::util::*;
use crate::Result;
use anchor_lang::{prelude::*, system_program};
#[cfg(feature = "enable-log")]
use std::convert::identity;

pub const TICK_ARRAY_SEED: &str = "tick_array";
pub const TICK_ARRAY_SIZE_USIZE: usize = 60;
pub const TICK_ARRAY_SIZE: i32 = 60;

/// Result of limit order matching
#[derive(Debug, Clone, Copy, Default)]
pub struct LimitOrderMatchResult {
    /// Amount of input tokens consumed by the limit order
    pub amount_in: u64,
    /// Amount of output tokens produced by the limit order
    pub amount_out: u64,
    /// Amount of fee tokens paid by the swap taker
    pub amm_fee_amount: u64,
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct TickArrayState {
    pub pool_id: Pubkey,
    pub start_tick_index: i32,
    pub ticks: [TickState; TICK_ARRAY_SIZE_USIZE],
    pub initialized_tick_count: u8,
    // account update recent epoch
    pub recent_epoch: u64,
    // Unused bytes for future upgrades.
    pub padding: [u8; 107],
}

impl TickArrayState {
    pub const LEN: usize = 8 + 32 + 4 + TickState::LEN * TICK_ARRAY_SIZE_USIZE + 1 + 115;

    pub fn key(&self) -> Pubkey {
        Pubkey::find_program_address(
            &[
                TICK_ARRAY_SEED.as_bytes(),
                self.pool_id.as_ref(),
                &self.start_tick_index.to_be_bytes(),
            ],
            &crate::id(),
        )
        .0
    }
    /// Load a TickArrayState of type AccountLoader from tickarray account info, if tickarray account does not exist, then create it.
    pub fn get_or_create_tick_array<'info>(
        payer: AccountInfo<'info>,
        tick_array_account_info: AccountInfo<'info>,
        system_program: AccountInfo<'info>,
        pool_state: Pubkey,
        tick_array_start_index: i32,
        tick_spacing: u16,
    ) -> Result<AccountLoad<'info, TickArrayState>> {
        require!(
            TickArrayState::check_is_valid_start_index(tick_array_start_index, tick_spacing),
            ErrorCode::InvalidTickIndex
        );

        let tick_array_state = if tick_array_account_info.owner == &system_program::ID {
            let (expect_pda_address, bump) = Pubkey::find_program_address(
                &[
                    TICK_ARRAY_SEED.as_bytes(),
                    pool_state.key().as_ref(),
                    &tick_array_start_index.to_be_bytes(),
                ],
                &crate::id(),
            );
            require_keys_eq!(expect_pda_address, tick_array_account_info.key());
            create_or_allocate_account(
                &crate::id(),
                payer,
                system_program,
                tick_array_account_info.clone(),
                &[
                    TICK_ARRAY_SEED.as_bytes(),
                    pool_state.key().as_ref(),
                    &tick_array_start_index.to_be_bytes(),
                    &[bump],
                ],
                TickArrayState::LEN,
            )?;
            let tick_array_state_loader = AccountLoad::<TickArrayState>::try_from_unchecked(
                &crate::id(),
                &tick_array_account_info,
            )?;
            {
                let mut tick_array_account = tick_array_state_loader.load_init()?;
                tick_array_account.initialize(
                    tick_array_start_index,
                    tick_spacing,
                    pool_state.key(),
                )?;
            }
            tick_array_state_loader
        } else {
            AccountLoad::<TickArrayState>::try_from(&tick_array_account_info)?
        };
        Ok(tick_array_state)
    }

    /**
     * Initialize only can be called when first created
     */
    pub fn initialize(
        &mut self,
        start_index: i32,
        tick_spacing: u16,
        pool_key: Pubkey,
    ) -> Result<()> {
        TickArrayState::check_is_valid_start_index(start_index, tick_spacing);
        self.start_tick_index = start_index;
        self.pool_id = pool_key;
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }

    pub fn update_initialized_tick_count(&mut self, add: bool) -> Result<()> {
        if add {
            self.initialized_tick_count = self
                .initialized_tick_count
                .checked_add(1)
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else {
            self.initialized_tick_count = self
                .initialized_tick_count
                .checked_sub(1)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }
        Ok(())
    }

    pub fn get_tick_state_mut(
        &mut self,
        tick_index: i32,
        tick_spacing: u16,
    ) -> Result<&mut TickState> {
        let offset_in_array = self.get_tick_offset_in_array(tick_index, tick_spacing)?;
        Ok(&mut self.ticks[offset_in_array])
    }

    pub fn get_tick_state(&self, tick_index: i32, tick_spacing: u16) -> Result<&TickState> {
        let offset_in_array = self.get_tick_offset_in_array(tick_index, tick_spacing)?;
        Ok(&self.ticks[offset_in_array])
    }

    pub fn update_tick_state(
        &mut self,
        tick_index: i32,
        tick_spacing: u16,
        tick_state: TickState,
    ) -> Result<()> {
        let offset_in_array = self.get_tick_offset_in_array(tick_index, tick_spacing)?;
        self.ticks[offset_in_array] = tick_state;
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }

    /// Get tick's offset in current tick array, tick must be include in tick array， otherwise throw an error
    fn get_tick_offset_in_array(self, tick_index: i32, tick_spacing: u16) -> Result<usize> {
        let start_tick_index = TickArrayState::get_array_start_index(tick_index, tick_spacing);
        require_eq!(
            start_tick_index,
            self.start_tick_index,
            ErrorCode::InvalidTickArray
        );
        let offset_in_array =
            ((tick_index - self.start_tick_index) / i32::from(tick_spacing)) as usize;
        Ok(offset_in_array)
    }

    /// Base on swap directioin, return the first initialized tick in the tick array.
    pub fn first_initialized_tick(&mut self, zero_for_one: bool) -> Result<&mut TickState> {
        if zero_for_one {
            self.ticks
                .iter_mut()
                .rev()
                .find(|tick| tick.is_initialized())
                .ok_or_else(|| error!(ErrorCode::InvalidTickArray))
        } else {
            self.ticks
                .iter_mut()
                .find(|tick| tick.is_initialized())
                .ok_or_else(|| error!(ErrorCode::InvalidTickArray))
        }
    }

    /// Get next initialized tick in tick array, `current_tick_index` can be any tick index, in other words, `current_tick_index` not exactly a point in the tickarray,
    /// and current_tick_index % tick_spacing maybe not equal zero.
    /// If price move to left tick <= current_tick_index, or to right tick > current_tick_index
    pub fn next_initialized_tick(
        &mut self,
        current_tick_index: i32,
        tick_spacing: u16,
        zero_for_one: bool,
    ) -> Result<Option<&mut TickState>> {
        let current_tick_array_start_index =
            TickArrayState::get_array_start_index(current_tick_index, tick_spacing);
        if current_tick_array_start_index != self.start_tick_index {
            return Ok(None);
        }
        let offset_in_array =
            (current_tick_index - self.start_tick_index) / i32::from(tick_spacing);

        let found_index = if zero_for_one {
            (0..=offset_in_array)
                .rev()
                .find(|&i| self.ticks[i as usize].is_initialized())
        } else {
            ((offset_in_array + 1)..TICK_ARRAY_SIZE)
                .find(|&i| self.ticks[i as usize].is_initialized())
        };
        Ok(found_index.map(|i| &mut self.ticks[i as usize]))
    }

    /// Base on swap directioin, return the next tick array start index.
    pub fn next_tick_arrary_start_index(&self, tick_spacing: u16, zero_for_one: bool) -> i32 {
        let ticks_in_array = TICK_ARRAY_SIZE * i32::from(tick_spacing);
        if zero_for_one {
            self.start_tick_index - ticks_in_array
        } else {
            self.start_tick_index + ticks_in_array
        }
    }

    /// Input an arbitrary tick_index, output the start_index of the tick_array it sits on
    pub fn get_array_start_index(tick_index: i32, tick_spacing: u16) -> i32 {
        let ticks_in_array = TickArrayState::tick_count(tick_spacing);
        let mut start = tick_index / ticks_in_array;
        if tick_index < 0 && tick_index % ticks_in_array != 0 {
            start = start - 1
        }
        start * ticks_in_array
    }

    pub fn check_is_valid_start_index(tick_index: i32, tick_spacing: u16) -> bool {
        if TickState::check_is_out_of_boundary(tick_index) {
            if tick_index > tick_math::MAX_TICK {
                return false;
            }
            let min_start_index =
                TickArrayState::get_array_start_index(tick_math::MIN_TICK, tick_spacing);
            return tick_index == min_start_index;
        }
        tick_index % TickArrayState::tick_count(tick_spacing) == 0
    }

    pub fn tick_count(tick_spacing: u16) -> i32 {
        TICK_ARRAY_SIZE * i32::from(tick_spacing)
    }
}

impl Default for TickArrayState {
    #[inline]
    fn default() -> TickArrayState {
        TickArrayState {
            pool_id: Pubkey::default(),
            ticks: [TickState::default(); TICK_ARRAY_SIZE_USIZE],
            start_tick_index: 0,
            initialized_tick_count: 0,
            recent_epoch: 0,
            padding: [0; 107],
        }
    }
}

#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct TickState {
    pub tick: i32,
    /// Amount of net liquidity added (subtracted) when tick is crossed from left to right (right to left)
    pub liquidity_net: i128,
    /// The total position liquidity that references this tick
    pub liquidity_gross: u128,

    /// Fee growth per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub fee_growth_outside_0_x64: u128,
    pub fee_growth_outside_1_x64: u128,

    /// Reward growth per unit of liquidity like fee, array of Q64.64
    pub reward_growths_outside_x64: [u128; REWARD_NUM],

    // Limit order related fields
    /// Order phase of the tick, used as a FIFO cohort index for limit orders
    pub order_phase: u64,
    /// The amount of limit orders that have never been matched,
    /// only counts newly opened orders, not partially filled ones
    pub orders_amount: u64,
    /// Remaining part filled orders amount
    pub part_filled_orders_remaining: u64,
    /// Cumulative unfilled ratio for the current part-filled cohort (Q64.64 format).
    /// Starts at Q64(1) when a new cohort forms, multiplied down as fills occur.
    pub unfilled_ratio_x64: u128,
    pub padding: [u32; 3],
}

impl TickState {
    pub const LEN: usize = 4 + 16 + 16 + 16 + 16 + 16 * REWARD_NUM + 4 * 8 + 16 + 4 * 1;

    pub fn initialize(&mut self, tick: i32, tick_spacing: u16) -> Result<()> {
        if TickState::check_is_out_of_boundary(tick) {
            return err!(ErrorCode::InvalidTickIndex);
        }
        require!(
            tick % i32::from(tick_spacing) == 0,
            ErrorCode::TickAndSpacingNotMatch
        );
        self.tick = tick;
        Ok(())
    }
    /// Updates a tick and returns true if the tick was flipped from initialized to uninitialized
    pub fn update(
        &mut self,
        tick_current: i32,
        liquidity_delta: i128,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        upper: bool,
        reward_infos: &[RewardInfo; REWARD_NUM],
    ) -> Result<bool> {
        let liquidity_gross_before = self.liquidity_gross;
        let liquidity_gross_after =
            liquidity_math::add_delta(liquidity_gross_before, liquidity_delta)?;

        // Either liquidity_gross_after becomes 0 (uninitialized) XOR liquidity_gross_before
        // was zero (initialized), and there are no unfilled limit orders
        let no_unfilled_orders = self.limit_order_unfilled_amount()? == 0;
        let flipped =
            ((liquidity_gross_after == 0) != (liquidity_gross_before == 0)) && no_unfilled_orders;
        if liquidity_gross_before == 0 {
            // by convention, we assume that all growth before a tick was initialized happened _below_ the tick
            if self.tick <= tick_current {
                self.fee_growth_outside_0_x64 = fee_growth_global_0_x64;
                self.fee_growth_outside_1_x64 = fee_growth_global_1_x64;
                self.reward_growths_outside_x64 = RewardInfo::get_reward_growths(reward_infos);
            }
        }

        self.liquidity_gross = liquidity_gross_after;

        // when the lower (upper) tick is crossed left to right (right to left),
        // liquidity must be added (removed)
        self.liquidity_net = if upper {
            self.liquidity_net.checked_sub(liquidity_delta)
        } else {
            self.liquidity_net.checked_add(liquidity_delta)
        }
        .ok_or(ErrorCode::CalculateOverflow)?;
        Ok(flipped)
    }

    /// Transitions to the current tick as needed by price movement, returning the amount of liquidity
    /// added (subtracted) when tick is crossed from left to right (right to left)
    pub fn cross(
        &mut self,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        reward_infos: &[RewardInfo; REWARD_NUM],
    ) -> i128 {
        self.fee_growth_outside_0_x64 =
            fee_growth_global_0_x64.wrapping_sub(self.fee_growth_outside_0_x64);
        self.fee_growth_outside_1_x64 =
            fee_growth_global_1_x64.wrapping_sub(self.fee_growth_outside_1_x64);

        for i in 0..REWARD_NUM {
            if !reward_infos[i].initialized() {
                continue;
            }

            self.reward_growths_outside_x64[i] = reward_infos[i]
                .reward_growth_global_x64
                .wrapping_sub(self.reward_growths_outside_x64[i]);
        }

        self.liquidity_net
    }

    pub fn clear(&mut self) {
        self.liquidity_net = 0;
        self.liquidity_gross = 0;
        self.fee_growth_outside_0_x64 = 0;
        self.fee_growth_outside_1_x64 = 0;
        self.reward_growths_outside_x64 = [0; REWARD_NUM];
    }

    pub fn is_initialized(&self) -> bool {
        self.has_liquidity() || self.has_limit_orders()
    }

    pub fn has_limit_orders(&self) -> bool {
        self.orders_amount > 0 || self.part_filled_orders_remaining > 0
    }

    pub fn has_liquidity(&self) -> bool {
        self.liquidity_gross > 0
    }
    /// Get the output amount of a limit order
    /// amount_in: the amount of the input token
    /// zero_for_one: the direction of the input token
    /// output amount is rounded down
    pub fn get_limit_order_output(amount_in: u64, tick: i32, zero_for_one: bool) -> Result<u64> {
        let output_amount = if zero_for_one {
            let token_0_price_x64 = tick_math::get_price_at_tick(tick, false)?;
            // Convert token0 amount to token1 amount using token0 price
            // token1_amount = token0_amount * token_0_price_x64 / 2^64
            U128::from(amount_in)
                .mul_div_floor(token_0_price_x64, U128::from(fixed_point_64::Q64))
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64()
        } else {
            let token_0_price_x64 = tick_math::get_price_at_tick(tick, true)?;
            // Convert token1 amount to token0 amount using token1 price (1/token_0_price_x64)
            // token0_amount = token1_amount * 2^64 / token_0_price_x64
            U128::from(amount_in)
                .mul_div_floor(U128::from(fixed_point_64::Q64), token_0_price_x64)
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64()
        };
        Ok(output_amount)
    }

    /// Given the output amount from a limit order, calculate the required input token amount
    /// the direction of the limit order is always opposite to the direction of the swap
    /// amount_out: the amount of the output token(limit order token)
    /// zero_for_one: the direction of the limit order
    /// input amount is rounded up
    pub fn get_limit_order_input(amount_out: u64, tick: i32, zero_for_one: bool) -> Result<u64> {
        let amount_in = if zero_for_one {
            let token_0_price_x64 = tick_math::get_price_at_tick(tick, true)?;
            // token1_consumed = token0_executed * token_0_price_x64 / 2^64
            U128::from(amount_out)
                .mul_div_ceil(token_0_price_x64, U128::from(fixed_point_64::Q64))
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64()
        } else {
            let token_0_price_x64 = tick_math::get_price_at_tick(tick, false)?;
            // token0_consumed = token1_executed * 2^64 / token_0_price_x64
            U128::from(amount_out)
                .mul_div_ceil(U128::from(fixed_point_64::Q64), token_0_price_x64)
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64()
        };
        Ok(amount_in)
    }

    pub fn limit_order_unfilled_amount(&self) -> Result<u64> {
        let total_unfilled_amount = self
            .orders_amount
            .checked_add(self.part_filled_orders_remaining)
            .ok_or(ErrorCode::CalculateOverflow)?;
        Ok(total_unfilled_amount)
    }

    pub fn match_limit_order(
        &mut self,
        swap_amount: u64,
        swap_direction_zero_for_one: bool,
        is_base_input: bool,
        fee_rate: u32,
        is_fee_on_input: bool,
    ) -> Result<LimitOrderMatchResult> {
        let mut result = LimitOrderMatchResult::default();

        let total_unfilled_amount = self.limit_order_unfilled_amount()?;
        if swap_amount == 0 || total_unfilled_amount == 0 {
            return Ok(result);
        }

        if is_base_input {
            // Assume the input amount can be fully consumed, calculate the amount of limit order tokens matched
            if is_fee_on_input {
                result.amm_fee_amount = swap_amount
                    .mul_div_ceil((fee_rate).into(), u64::from(FEE_RATE_DENOMINATOR_VALUE))
                    .ok_or(ErrorCode::CalculateOverflow)?;
                result.amount_in = swap_amount - result.amm_fee_amount;
            } else {
                result.amount_in = swap_amount;
            }
            result.amount_out = TickState::get_limit_order_output(
                result.amount_in,
                self.tick,
                swap_direction_zero_for_one,
            )?;
            // If the amount of limit order tokens matched is greater than the total unfilled amount,
            // it means the input cannot be fully consumed, so recalculate the input and output amounts
            if result.amount_out > total_unfilled_amount {
                result.amount_out = total_unfilled_amount;
                result.amount_in = TickState::get_limit_order_input(
                    total_unfilled_amount,
                    self.tick,
                    !swap_direction_zero_for_one,
                )?;
                if is_fee_on_input {
                    result.amm_fee_amount = result
                        .amount_in
                        .mul_div_ceil(
                            (fee_rate).into(),
                            u64::from(FEE_RATE_DENOMINATOR_VALUE - fee_rate),
                        )
                        .ok_or(ErrorCode::CalculateOverflow)?;
                }
                // Fee from output will be calculated at the end
            }
        } else {
            // swap_amount is the desired net output (after fee deduction if fee is from output)
            let net_output = swap_amount.min(total_unfilled_amount);
            result.amount_out = if is_fee_on_input {
                net_output
            } else {
                // total_output = net_output / (1 - fee_rate / FEE_RATE_DENOMINATOR)
                net_output
                    .mul_div_ceil(
                        u64::from(FEE_RATE_DENOMINATOR_VALUE).into(),
                        (FEE_RATE_DENOMINATOR_VALUE - fee_rate).into(),
                    )
                    .ok_or(ErrorCode::CalculateOverflow)?
                    .min(total_unfilled_amount)
            };
            result.amount_in = TickState::get_limit_order_input(
                result.amount_out,
                self.tick,
                !swap_direction_zero_for_one,
            )?;
            if is_fee_on_input {
                result.amm_fee_amount = result
                    .amount_in
                    .mul_div_ceil(
                        (fee_rate).into(),
                        u64::from(FEE_RATE_DENOMINATOR_VALUE - fee_rate),
                    )
                    .ok_or(ErrorCode::CalculateOverflow)?;
            }
            // Fee from output will be calculated at the end
        }

        let mut consume_from_part_remaining = 0;
        // Consume part_filled_orders_remaining first (FIFO priority)
        if self.part_filled_orders_remaining > 0 {
            consume_from_part_remaining = self.part_filled_orders_remaining.min(result.amount_out);
            // Update unfilled_ratio: ratio *= (remaining - consumed) / remaining
            if consume_from_part_remaining > 0 {
                self.unfilled_ratio_x64 = U128::from(self.unfilled_ratio_x64)
                    .mul_div_floor(
                        U128::from(self.part_filled_orders_remaining - consume_from_part_remaining),
                        U128::from(self.part_filled_orders_remaining),
                    )
                    .ok_or(ErrorCode::CalculateOverflow)?
                    .as_u128();
            }
            self.part_filled_orders_remaining = self
                .part_filled_orders_remaining
                .saturating_sub(consume_from_part_remaining);
        }
        let amount_out_continue_to_consume = result
            .amount_out
            .saturating_sub(consume_from_part_remaining);

        // If there is still more to consume, consume from orders_amount
        if amount_out_continue_to_consume > 0 {
            require_eq!(self.part_filled_orders_remaining, 0);
            require_gte!(
                self.orders_amount,
                amount_out_continue_to_consume,
                ErrorCode::InvalidLimitOrderAmount
            );
            // Order phase increases when consuming from orders_amount
            self.order_phase = self.order_phase.saturating_add(1);

            // Reset unfilled_ratio for new phase, then update for consumption
            self.unfilled_ratio_x64 = U128::from(fixed_point_64::Q64)
                .mul_div_floor(
                    U128::from(self.orders_amount - amount_out_continue_to_consume),
                    U128::from(self.orders_amount),
                )
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u128();

            // Move remaining orders_amount to part_filled_orders_remaining
            self.part_filled_orders_remaining = self.orders_amount - amount_out_continue_to_consume;
            self.orders_amount = 0;
        }
        // Calculate fee and deduct from output if fee is from output (after limit order consumption calculation)
        // Limit order consumption calculation needs gross output, so we calculate and deduct fee at the end
        if !is_fee_on_input {
            result.amm_fee_amount = result
                .amount_out
                .mul_div_ceil((fee_rate).into(), u64::from(FEE_RATE_DENOMINATOR_VALUE))
                .ok_or(ErrorCode::CalculateOverflow)?;
            // Deduct fee from output: user receives net output
            result.amount_out = result
                .amount_out
                .checked_sub(result.amm_fee_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;
        }
        Ok(result)
    }

    /// Common checks for a valid tick input.
    /// A tick is valid if it lies within tick boundaries
    pub fn check_is_out_of_boundary(tick: i32) -> bool {
        tick < tick_math::MIN_TICK || tick > tick_math::MAX_TICK
    }
}

// Calculates the fee growths inside of tick_lower and tick_upper based on their positions relative to tick_current.
/// `fee_growth_inside = fee_growth_global - fee_growth_below(lower) - fee_growth_above(upper)`
///
pub fn get_fee_growth_inside(
    tick_lower: &TickState,
    tick_upper: &TickState,
    tick_current: i32,
    fee_growth_global_0_x64: u128,
    fee_growth_global_1_x64: u128,
) -> (u128, u128) {
    // calculate fee growth below
    let (fee_growth_below_0_x64, fee_growth_below_1_x64) = if tick_current >= tick_lower.tick {
        (
            tick_lower.fee_growth_outside_0_x64,
            tick_lower.fee_growth_outside_1_x64,
        )
    } else {
        (
            fee_growth_global_0_x64.wrapping_sub(tick_lower.fee_growth_outside_0_x64),
            fee_growth_global_1_x64.wrapping_sub(tick_lower.fee_growth_outside_1_x64),
        )
    };

    // Calculate fee growth above
    let (fee_growth_above_0_x64, fee_growth_above_1_x64) = if tick_current < tick_upper.tick {
        (
            tick_upper.fee_growth_outside_0_x64,
            tick_upper.fee_growth_outside_1_x64,
        )
    } else {
        (
            fee_growth_global_0_x64.wrapping_sub(tick_upper.fee_growth_outside_0_x64),
            fee_growth_global_1_x64.wrapping_sub(tick_upper.fee_growth_outside_1_x64),
        )
    };
    let fee_growth_inside_0_x64 = fee_growth_global_0_x64
        .wrapping_sub(fee_growth_below_0_x64)
        .wrapping_sub(fee_growth_above_0_x64);
    let fee_growth_inside_1_x64 = fee_growth_global_1_x64
        .wrapping_sub(fee_growth_below_1_x64)
        .wrapping_sub(fee_growth_above_1_x64);

    (fee_growth_inside_0_x64, fee_growth_inside_1_x64)
}

// Calculates the reward growths inside of tick_lower and tick_upper based on their positions relative to tick_current.
pub fn get_reward_growths_inside(
    tick_lower: &TickState,
    tick_upper: &TickState,
    tick_current_index: i32,
    reward_infos: &[RewardInfo; REWARD_NUM],
) -> [u128; REWARD_NUM] {
    let mut reward_growths_inside = [0; REWARD_NUM];

    for i in 0..REWARD_NUM {
        if !reward_infos[i].initialized() {
            continue;
        }

        let reward_growths_below = if tick_current_index >= tick_lower.tick {
            tick_lower.reward_growths_outside_x64[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .wrapping_sub(tick_lower.reward_growths_outside_x64[i])
        };

        let reward_growths_above = if tick_current_index < tick_upper.tick {
            tick_upper.reward_growths_outside_x64[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .wrapping_sub(tick_upper.reward_growths_outside_x64[i])
        };
        reward_growths_inside[i] = reward_infos[i]
            .reward_growth_global_x64
            .wrapping_sub(reward_growths_below)
            .wrapping_sub(reward_growths_above);
        #[cfg(feature = "enable-log")]
        msg!(
            "get_reward_growths_inside,i:{},reward_growth_global:{},reward_growth_below:{},reward_growth_above:{}, reward_growth_inside:{}",
            i,
            identity(reward_infos[i].reward_growth_global_x64),
            reward_growths_below,
            reward_growths_above,
            reward_growths_inside[i]
        );
    }

    reward_growths_inside
}

pub fn check_tick_array_start_index(
    tick_array_start_index: i32,
    tick_index: i32,
    tick_spacing: u16,
) -> Result<()> {
    require!(
        tick_index >= tick_math::MIN_TICK,
        ErrorCode::TickLowerOverflow
    );
    require!(
        tick_index <= tick_math::MAX_TICK,
        ErrorCode::TickUpperOverflow
    );
    require_eq!(0, tick_index % i32::from(tick_spacing));
    let expect_start_index = TickArrayState::get_array_start_index(tick_index, tick_spacing);
    require_eq!(tick_array_start_index, expect_start_index);
    Ok(())
}

/// Common checks for valid tick inputs.
///
pub fn check_ticks_order(tick_lower_index: i32, tick_upper_index: i32) -> Result<()> {
    require!(
        tick_lower_index < tick_upper_index,
        ErrorCode::TickInvalidOrder
    );
    Ok(())
}

#[cfg(test)]
pub mod tick_array_test {
    use super::*;
    use std::cell::RefCell;

    pub struct TickArrayInfo {
        pub start_tick_index: i32,
        pub ticks: Vec<TickState>,
    }

    pub fn build_tick_array(
        start_index: i32,
        tick_spacing: u16,
        initialized_tick_offsets: Vec<usize>,
    ) -> RefCell<TickArrayState> {
        let mut new_tick_array = TickArrayState::default();
        new_tick_array
            .initialize(start_index, tick_spacing, Pubkey::default())
            .unwrap();

        for offset in initialized_tick_offsets {
            let mut new_tick = TickState::default();
            // Indicates tick is initialized
            new_tick.liquidity_gross = 1;
            new_tick.tick = start_index + (offset * tick_spacing as usize) as i32;
            new_tick_array.ticks[offset] = new_tick;
        }
        RefCell::new(new_tick_array)
    }

    pub fn build_tick_array_with_tick_states(
        pool_id: Pubkey,
        start_index: i32,
        tick_spacing: u16,
        tick_states: Vec<TickState>,
    ) -> RefCell<TickArrayState> {
        let mut new_tick_array = TickArrayState::default();
        new_tick_array
            .initialize(start_index, tick_spacing, pool_id)
            .unwrap();
        new_tick_array.initialized_tick_count = tick_states.len() as u8;
        for tick_state in tick_states {
            let offset = new_tick_array
                .get_tick_offset_in_array(tick_state.tick, tick_spacing)
                .unwrap();
            new_tick_array.ticks[offset] = tick_state;
        }
        RefCell::new(new_tick_array)
    }

    pub fn build_tick(tick: i32, liquidity_gross: u128, liquidity_net: i128) -> RefCell<TickState> {
        let mut new_tick = TickState::default();
        new_tick.tick = tick;
        new_tick.liquidity_gross = liquidity_gross;
        new_tick.liquidity_net = liquidity_net;
        RefCell::new(new_tick)
    }

    fn build_tick_with_fee_reward_growth(
        tick: i32,
        fee_growth_outside_0_x64: u128,
        fee_growth_outside_1_x64: u128,
        reward_growths_outside_x64: u128,
    ) -> RefCell<TickState> {
        let mut new_tick = TickState::default();
        new_tick.tick = tick;
        new_tick.fee_growth_outside_0_x64 = fee_growth_outside_0_x64;
        new_tick.fee_growth_outside_1_x64 = fee_growth_outside_1_x64;
        new_tick.reward_growths_outside_x64 = [reward_growths_outside_x64, 0, 0];
        RefCell::new(new_tick)
    }

    mod tick_array_test {
        use super::*;
        use std::convert::identity;

        #[test]
        fn get_array_start_index_test() {
            assert_eq!(TickArrayState::get_array_start_index(120, 3), 0);
            assert_eq!(TickArrayState::get_array_start_index(1002, 30), 0);
            assert_eq!(TickArrayState::get_array_start_index(-120, 3), -180);
            assert_eq!(TickArrayState::get_array_start_index(-1002, 30), -1800);
            assert_eq!(TickArrayState::get_array_start_index(-20, 10), -600);
            assert_eq!(TickArrayState::get_array_start_index(20, 10), 0);
            assert_eq!(TickArrayState::get_array_start_index(-1002, 10), -1200);
            assert_eq!(TickArrayState::get_array_start_index(-600, 10), -600);
            assert_eq!(TickArrayState::get_array_start_index(-30720, 1), -30720);
            assert_eq!(TickArrayState::get_array_start_index(30720, 1), 30720);
            assert_eq!(
                TickArrayState::get_array_start_index(tick_math::MIN_TICK, 1),
                -443640
            );
            assert_eq!(
                TickArrayState::get_array_start_index(tick_math::MAX_TICK, 1),
                443580
            );
            assert_eq!(
                TickArrayState::get_array_start_index(tick_math::MAX_TICK, 60),
                442800
            );
            assert_eq!(
                TickArrayState::get_array_start_index(tick_math::MIN_TICK, 60),
                -446400
            );
        }

        #[test]
        fn next_tick_arrary_start_index_test() {
            let tick_spacing = 15;
            let tick_array_ref = build_tick_array(-1800, tick_spacing, vec![]);
            // zero_for_one, next tickarray start_index < current
            assert_eq!(
                -2700,
                tick_array_ref
                    .borrow()
                    .next_tick_arrary_start_index(tick_spacing, true)
            );
            // one_for_zero, next tickarray start_index > current
            assert_eq!(
                -900,
                tick_array_ref
                    .borrow()
                    .next_tick_arrary_start_index(tick_spacing, false)
            );
        }

        #[test]
        fn get_tick_offset_in_array_test() {
            let tick_spacing = 4;
            // tick range [960, 1196]
            let tick_array_ref = build_tick_array(960, tick_spacing, vec![]);

            // not in tickarray
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(808, tick_spacing)
                    .unwrap_err(),
                error!(ErrorCode::InvalidTickArray)
            );
            // first index is tickarray start tick
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(960, tick_spacing)
                    .unwrap(),
                0
            );
            // tick_index % tick_spacing != 0
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(1105, tick_spacing)
                    .unwrap(),
                36
            );
            // (1108-960) / tick_spacing
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(1108, tick_spacing)
                    .unwrap(),
                37
            );
            // the end index of tickarray
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(1196, tick_spacing)
                    .unwrap(),
                59
            );
        }

        #[test]
        fn first_initialized_tick_test() {
            let tick_spacing = 15;
            // initialized ticks[-300,-15]
            let tick_array_ref = build_tick_array(-900, tick_spacing, vec![40, 59]);
            let mut tick_array = tick_array_ref.borrow_mut();
            // one_for_zero, the price increase, tick from small to large
            let tick = tick_array.first_initialized_tick(false).unwrap().tick;
            assert_eq!(-300, tick);
            // zero_for_one, the price decrease, tick from large to small
            let tick = tick_array.first_initialized_tick(true).unwrap().tick;
            assert_eq!(-15, tick);
        }

        #[test]
        fn next_initialized_tick_when_tick_is_positive() {
            // init tick_index [0,30,105]
            let tick_array_ref = build_tick_array(0, 15, vec![0, 2, 7]);
            let mut tick_array = tick_array_ref.borrow_mut();

            // test zero_for_one
            let mut next_tick_state = tick_array.next_initialized_tick(0, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);

            next_tick_state = tick_array.next_initialized_tick(1, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);

            next_tick_state = tick_array.next_initialized_tick(29, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);
            next_tick_state = tick_array.next_initialized_tick(30, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);
            next_tick_state = tick_array.next_initialized_tick(31, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);

            // test one for zero
            let mut next_tick_state = tick_array.next_initialized_tick(0, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);

            next_tick_state = tick_array.next_initialized_tick(29, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);
            next_tick_state = tick_array.next_initialized_tick(30, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 105);
            next_tick_state = tick_array.next_initialized_tick(31, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 105);

            next_tick_state = tick_array.next_initialized_tick(105, 15, false).unwrap();
            assert!(next_tick_state.is_none());

            // tick not in tickarray
            next_tick_state = tick_array.next_initialized_tick(900, 15, false).unwrap();
            assert!(next_tick_state.is_none());
        }

        #[test]
        fn next_initialized_tick_when_tick_is_negative() {
            // init tick_index [-900,-870,-795]
            let tick_array_ref = build_tick_array(-900, 15, vec![0, 2, 7]);
            let mut tick_array = tick_array_ref.borrow_mut();

            // test zero for one
            let mut next_tick_state = tick_array.next_initialized_tick(-900, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);

            next_tick_state = tick_array.next_initialized_tick(-899, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);

            next_tick_state = tick_array.next_initialized_tick(-871, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);
            next_tick_state = tick_array.next_initialized_tick(-870, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);
            next_tick_state = tick_array.next_initialized_tick(-869, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);

            // test one for zero
            let mut next_tick_state = tick_array.next_initialized_tick(-900, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);

            next_tick_state = tick_array.next_initialized_tick(-871, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);
            next_tick_state = tick_array.next_initialized_tick(-870, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -795);
            next_tick_state = tick_array.next_initialized_tick(-869, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -795);

            next_tick_state = tick_array.next_initialized_tick(-795, 15, false).unwrap();
            assert!(next_tick_state.is_none());

            // tick not in tickarray
            next_tick_state = tick_array.next_initialized_tick(-10, 15, false).unwrap();
            assert!(next_tick_state.is_none());
        }
    }

    mod get_fee_growth_inside_test {
        use super::*;
        use crate::states::{
            pool::RewardInfo,
            tick_array::{get_fee_growth_inside, TickState},
        };

        fn fee_growth_inside_delta_when_price_move(
            init_fee_growth_global_0_x64: u128,
            init_fee_growth_global_1_x64: u128,
            fee_growth_global_delta: u128,
            mut tick_current: i32,
            target_tick_current: i32,
            tick_lower: &mut TickState,
            tick_upper: &mut TickState,
            cross_tick_lower: bool,
        ) -> (u128, u128) {
            let mut fee_growth_global_0_x64 = init_fee_growth_global_0_x64;
            let mut fee_growth_global_1_x64 = init_fee_growth_global_1_x64;
            let (fee_growth_inside_0_before, fee_growth_inside_1_before) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );

            if fee_growth_global_0_x64 != 0 {
                fee_growth_global_0_x64 = fee_growth_global_0_x64 + fee_growth_global_delta;
            }
            if fee_growth_global_1_x64 != 0 {
                fee_growth_global_1_x64 = fee_growth_global_1_x64 + fee_growth_global_delta;
            }
            if cross_tick_lower {
                tick_lower.cross(
                    fee_growth_global_0_x64,
                    fee_growth_global_1_x64,
                    &[RewardInfo::default(); 3],
                );
            } else {
                tick_upper.cross(
                    fee_growth_global_0_x64,
                    fee_growth_global_1_x64,
                    &[RewardInfo::default(); 3],
                );
            }

            tick_current = target_tick_current;
            let (fee_growth_inside_0_after, fee_growth_inside_1_after) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );

            println!(
                "inside_delta_0:{},fee_growth_inside_0_after:{},fee_growth_inside_0_before:{}",
                fee_growth_inside_0_after.wrapping_sub(fee_growth_inside_0_before),
                fee_growth_inside_0_after,
                fee_growth_inside_0_before
            );
            println!(
                "inside_delta_1:{},fee_growth_inside_1_after:{},fee_growth_inside_1_before:{}",
                fee_growth_inside_1_after.wrapping_sub(fee_growth_inside_1_before),
                fee_growth_inside_1_after,
                fee_growth_inside_1_before
            );
            (
                fee_growth_inside_0_after.wrapping_sub(fee_growth_inside_0_before),
                fee_growth_inside_1_after.wrapping_sub(fee_growth_inside_1_before),
            )
        }

        #[test]
        fn price_in_tick_range_move_to_right_test() {
            // one_for_zero, price move to right and token_1 fee growth

            // tick_lower and tick_upper all new create, and tick_lower initialize with fee_growth_global_1_x64(1000)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    0,
                    11,
                    build_tick_with_fee_reward_growth(-10, 0, 1000, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 500);

            // tick_lower is initialized with fee_growth_outside_1_x64(100) and tick_upper is new create.
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    0,
                    11,
                    build_tick_with_fee_reward_growth(-10, 0, 100, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 500);

            // tick_lower is new create with fee_growth_global_1_x64(1000)  and tick_upper is initialized with fee_growth_outside_1_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    0,
                    11,
                    build_tick_with_fee_reward_growth(-10, 0, 1000, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 100, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 500);

            // tick_lower is initialized with fee_growth_outside_1_x64(50)  and tick_upper is initialized with fee_growth_outside_1_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    0,
                    11,
                    build_tick_with_fee_reward_growth(-10, 0, 50, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 100, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 500);
        }

        #[test]
        fn price_in_tick_range_move_to_left_test() {
            // zero_for_one, price move to left and token_0 fee growth

            // tick_lower and tick_upper all new create, and tick_lower initialize with fee_growth_global_0_x64(1000)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    0,
                    -11,
                    build_tick_with_fee_reward_growth(-10, 1000, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 500);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_0_x64(100) and tick_upper is new create.
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    0,
                    -11,
                    build_tick_with_fee_reward_growth(-10, 100, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 500);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is new create with fee_growth_global_0_x64(1000)  and tick_upper is initialized with fee_growth_outside_0_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    0,
                    -11,
                    build_tick_with_fee_reward_growth(-10, 1000, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 100, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 500);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_0_x64(50)  and tick_upper is initialized with fee_growth_outside_0_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    0,
                    -11,
                    build_tick_with_fee_reward_growth(-10, 50, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 100, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 500);
            assert_eq!(fee_growth_inside_delta_1, 0);
        }

        #[test]
        fn price_in_tick_range_left_move_to_right_test() {
            // one_for_zero, price move to right and token_1 fee growth

            // tick_lower and tick_upper all new create
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    -11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 0, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_1_x64(100) and tick_upper is new create.
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    -11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 0, 100, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is new create  and tick_upper is initialized with fee_growth_outside_1_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    -11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 0, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 100, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_1_x64(50)  and tick_upper is initialized with fee_growth_outside_1_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    0,
                    1000,
                    500,
                    -11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 0, 50, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 0, 100, 0).get_mut(),
                    true,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);
        }

        #[test]
        fn price_in_tick_range_right_move_to_left_test() {
            // zero_for_one, price move to left and token_0 fee growth

            // tick_lower and tick_upper all new create
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 1000, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 1000, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_0_x64(100) and tick_upper is new create.
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 100, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 1000, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is new create with fee_growth_global_0_x64(1000)  and tick_upper is initialized with fee_growth_outside_0_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 1000, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 100, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);

            // tick_lower is initialized with fee_growth_outside_0_x64(50)  and tick_upper is initialized with fee_growth_outside_0_x64(100)
            let (fee_growth_inside_delta_0, fee_growth_inside_delta_1) =
                fee_growth_inside_delta_when_price_move(
                    1000,
                    0,
                    500,
                    11,
                    0,
                    build_tick_with_fee_reward_growth(-10, 50, 0, 0).get_mut(),
                    build_tick_with_fee_reward_growth(10, 100, 0, 0).get_mut(),
                    false,
                );
            assert_eq!(fee_growth_inside_delta_0, 0);
            assert_eq!(fee_growth_inside_delta_1, 0);
        }
    }

    mod get_reward_growths_inside_test {
        use super::*;
        use crate::states::{
            pool::RewardInfo,
            tick_array::{get_reward_growths_inside, TickState},
        };
        use anchor_lang::prelude::Pubkey;

        fn build_reward_infos(reward_growth_global_x64: u128) -> [RewardInfo; 3] {
            [
                RewardInfo {
                    token_mint: Pubkey::new_unique(),
                    reward_growth_global_x64,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ]
        }

        fn reward_growth_inside_delta_when_price_move(
            init_reward_growth_global_x64: u128,
            reward_growth_global_delta: u128,
            mut tick_current: i32,
            target_tick_current: i32,
            tick_lower: &mut TickState,
            tick_upper: &mut TickState,
            cross_tick_lower: bool,
        ) -> u128 {
            let mut reward_growth_global_x64 = init_reward_growth_global_x64;
            let reward_growth_inside_before = get_reward_growths_inside(
                tick_lower,
                tick_upper,
                tick_current,
                &build_reward_infos(reward_growth_global_x64),
            )[0];

            reward_growth_global_x64 = reward_growth_global_x64 + reward_growth_global_delta;
            if cross_tick_lower {
                tick_lower.cross(0, 0, &build_reward_infos(reward_growth_global_x64));
            } else {
                tick_upper.cross(0, 0, &build_reward_infos(reward_growth_global_x64));
            }

            tick_current = target_tick_current;
            let reward_growth_inside_after = get_reward_growths_inside(
                tick_lower,
                tick_upper,
                tick_current,
                &build_reward_infos(reward_growth_global_x64),
            )[0];

            println!(
                "inside_delta:{}, reward_growth_inside_after:{}, reward_growth_inside_before:{}",
                reward_growth_inside_after.wrapping_sub(reward_growth_inside_before),
                reward_growth_inside_after,
                reward_growth_inside_before,
            );

            reward_growth_inside_after.wrapping_sub(reward_growth_inside_before)
        }

        #[test]
        fn uninitialized_reward_index_test() {
            let tick_current = 0;

            let tick_lower = &mut TickState {
                tick: -10,
                reward_growths_outside_x64: [1000, 0, 0],
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                reward_growths_outside_x64: [1000, 0, 0],
                ..Default::default()
            };

            let reward_infos = &[RewardInfo::default(); 3];
            let reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current, reward_infos);
            assert_eq!(reward_inside, [0; 3]);
        }

        #[test]
        fn price_in_tick_range_move_to_right_test() {
            // tick_lower and tick_upper all new create
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 1000).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                false,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is initialized with reward_growths_outside_x64(100) and tick_upper is new create.
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 100).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                false,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is new create with reward_growths_outside_x64(1000)  and tick_upper is initialized with reward_growths_outside_x64(100)
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 1000).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 100).get_mut(),
                false,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is initialized with reward_growths_outside_x64(50)  and tick_upper is initialized with reward_growths_outside_x64(100)
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 50).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 100).get_mut(),
                false,
            );
            assert_eq!(reward_frowth_inside_delta, 500);
        }

        #[test]
        fn price_in_tick_range_move_to_left_test() {
            // zero_for_one, cross tick_lower

            // tick_lower and tick_upper all new create, and tick_lower initialize with reward_growths_outside_x64(1000)
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                -11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 1000).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                true,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is initialized with reward_growths_outside_x64(100) and tick_upper is new create.
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                -11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 100).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 0).get_mut(),
                true,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is new create with reward_growths_outside_x64(1000)  and tick_upper is initialized with reward_growths_outside_x64(100)
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                -11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 1000).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 100).get_mut(),
                true,
            );
            assert_eq!(reward_frowth_inside_delta, 500);

            // tick_lower is initialized with reward_growths_outside_x64(50)  and tick_upper is initialized with reward_growths_outside_x64(100)
            let reward_frowth_inside_delta = reward_growth_inside_delta_when_price_move(
                1000,
                500,
                0,
                -11,
                build_tick_with_fee_reward_growth(-10, 0, 0, 50).get_mut(),
                build_tick_with_fee_reward_growth(10, 0, 0, 100).get_mut(),
                true,
            );
            assert_eq!(reward_frowth_inside_delta, 500);
        }
    }
    mod tick_array_layout_test {
        use super::*;
        use anchor_lang::Discriminator;
        #[test]
        fn test_tick_array_layout() {
            let pool_id = Pubkey::new_unique();
            let start_tick_index: i32 = 0x12345678;
            let initialized_tick_count: u8 = 0x12;
            let recent_epoch: u64 = 0x123456789abcdef0;
            let mut padding: [u8; 107] = [0u8; 107];
            let mut padding_data = [0u8; 107];
            for i in 0..107 {
                padding[i] = i as u8;
                padding_data[i] = i as u8;
            }

            let tick: i32 = 0x12345678;
            let liquidity_net: i128 = 0x11002233445566778899aabbccddeeff;
            let liquidity_gross: u128 = 0x11220033445566778899aabbccddeeff;
            let fee_growth_outside_0_x64: u128 = 0x11223300445566778899aabbccddeeff;
            let fee_growth_outside_1_x64: u128 = 0x11223344005566778899aabbccddeeff;
            let reward_growths_outside_x64: [u128; REWARD_NUM] = [
                0x11223344550066778899aabbccddeeff,
                0x11223344556600778899aabbccddeeff,
                0x11223344556677008899aabbccddeeff,
            ];

            // Limit order related fields
            let limit_order_amount: u64 = 0x1122334455667788;
            let total_weight: u64 = 0x8877665544332211;
            let filled_per_weight_zero_for_one_x64: u128 = 0x112233445566778899aabbccddeeff00;
            let filled_per_weight_one_for_zero_x64: u128 = 0x00112233445566778899aabbccddeeff;
            let cross_id: u32 = 0x12345678;

            let mut tick_data = [0u8; TickState::LEN];
            let mut offset = 0;
            tick_data[offset..offset + 4].copy_from_slice(&tick.to_le_bytes());
            offset += 4;
            tick_data[offset..offset + 16].copy_from_slice(&liquidity_net.to_le_bytes());
            offset += 16;
            tick_data[offset..offset + 16].copy_from_slice(&liquidity_gross.to_le_bytes());
            offset += 16;
            tick_data[offset..offset + 16].copy_from_slice(&fee_growth_outside_0_x64.to_le_bytes());
            offset += 16;
            tick_data[offset..offset + 16].copy_from_slice(&fee_growth_outside_1_x64.to_le_bytes());
            offset += 16;
            for i in 0..REWARD_NUM {
                tick_data[offset..offset + 16]
                    .copy_from_slice(&reward_growths_outside_x64[i].to_le_bytes());
                offset += 16;
            }
            // Limit order fields
            tick_data[offset..offset + 8].copy_from_slice(&limit_order_amount.to_le_bytes());
            offset += 8;
            tick_data[offset..offset + 8].copy_from_slice(&total_weight.to_le_bytes());
            offset += 8;
            tick_data[offset..offset + 16]
                .copy_from_slice(&filled_per_weight_zero_for_one_x64.to_le_bytes());
            offset += 16;
            tick_data[offset..offset + 16]
                .copy_from_slice(&filled_per_weight_one_for_zero_x64.to_le_bytes());
            offset += 16;
            tick_data[offset..offset + 4].copy_from_slice(&cross_id.to_le_bytes());
            offset += 4;

            assert_eq!(offset, tick_data.len());
            assert_eq!(tick_data.len(), core::mem::size_of::<TickState>());

            // serialize original data
            let mut tick_array_data = [0u8; TickArrayState::LEN];
            let mut offset = 0;
            tick_array_data[offset..offset + 8].copy_from_slice(&TickArrayState::DISCRIMINATOR);
            offset += 8;
            tick_array_data[offset..offset + 32].copy_from_slice(&pool_id.to_bytes());
            offset += 32;
            tick_array_data[offset..offset + 4].copy_from_slice(&start_tick_index.to_le_bytes());
            offset += 4;
            for _ in 0..TICK_ARRAY_SIZE_USIZE {
                tick_array_data[offset..offset + TickState::LEN].copy_from_slice(&tick_data);
                offset += TickState::LEN;
            }
            tick_array_data[offset..offset + 1]
                .copy_from_slice(&initialized_tick_count.to_le_bytes());
            offset += 1;
            tick_array_data[offset..offset + 8].copy_from_slice(&recent_epoch.to_le_bytes());
            offset += 8;
            tick_array_data[offset..offset + 107].copy_from_slice(&padding);
            offset += 107;

            // len check
            assert_eq!(offset, tick_array_data.len());
            assert_eq!(
                tick_array_data.len(),
                core::mem::size_of::<TickArrayState>() + 8
            );

            // deserialize original data
            let unpack_data: &TickArrayState = bytemuck::from_bytes(
                &tick_array_data[8..core::mem::size_of::<TickArrayState>() + 8],
            );

            // data check
            let unpack_pool_id = unpack_data.pool_id;
            assert_eq!(unpack_pool_id, pool_id);
            let unpack_start_tick_index = unpack_data.start_tick_index;
            assert_eq!(unpack_start_tick_index, start_tick_index);
            for tick_item in unpack_data.ticks {
                let unpack_tick = tick_item.tick;
                assert_eq!(unpack_tick, tick);
                let unpack_liquidity_net = tick_item.liquidity_net;
                assert_eq!(unpack_liquidity_net, liquidity_net);
                let unpack_liquidity_gross = tick_item.liquidity_gross;
                assert_eq!(unpack_liquidity_gross, liquidity_gross);
                let unpack_fee_growth_outside_0_x64 = tick_item.fee_growth_outside_0_x64;
                assert_eq!(unpack_fee_growth_outside_0_x64, fee_growth_outside_0_x64);
                let unpack_fee_growth_outside_1_x64 = tick_item.fee_growth_outside_1_x64;
                assert_eq!(unpack_fee_growth_outside_1_x64, fee_growth_outside_1_x64);
                let unpack_reward_growths_outside_x64 = tick_item.reward_growths_outside_x64;
                assert_eq!(
                    unpack_reward_growths_outside_x64,
                    reward_growths_outside_x64
                );
            }
            let unpack_initialized_tick_count = unpack_data.initialized_tick_count;
            assert_eq!(unpack_initialized_tick_count, initialized_tick_count);
            let unpack_recent_epoch = unpack_data.recent_epoch;
            assert_eq!(unpack_recent_epoch, recent_epoch);
            let unpack_padding = unpack_data.padding;
            assert_eq!(padding, unpack_padding);
        }
    }
}
