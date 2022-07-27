use crate::error::ErrorCode;
use crate::libraries::{
    big_num::U128, fixed_point_64, full_math::MulDiv, liquidity_math, swap_math, tick_math,
};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::iter::Iterator;
use std::ops::Neg;
use std::ops::{Deref, DerefMut};

pub struct SwapContext<'b, 'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The user token account for input token
    pub input_token_account: Account<'info, TokenAccount>,

    /// The user token account for output token
    pub output_token_account: Account<'info, TokenAccount>,

    /// The vault token account for input token
    pub input_vault: Account<'info, TokenAccount>,

    /// The vault token account for output token
    pub output_vault: Account<'info, TokenAccount>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// The factory state to read protocol fees
    pub amm_config: &'b Account<'info, AmmConfig>,

    /// The program account of the pool in which the swap will be performed
    pub pool_state: &'b mut Account<'info, PoolState>,

    pub tick_array_state: &'b mut AccountLoader<'info, TickArrayState>,

    /// The program account for the oracle observation
    pub observation_state: &'b mut AccountLoader<'info, ObservationState>,
}

pub struct SwapCache {
    // the protocol fee for the input token
    pub protocol_fee_rate: u32,
    // liquidity at the beginning of the swap
    pub liquidity_start: u128,
    // the timestamp of the current block
    pub block_timestamp: u32,
}

// the top level state of the swap, the results of which are recorded in storage at the end
#[derive(Debug)]
pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    pub amount_specified_remaining: i64,
    // the amount already swapped out/in of the output/input asset
    pub amount_calculated: i64,
    // current sqrt(price)
    pub sqrt_price_x64: u128,
    // the tick associated with the current price
    pub tick: i32,
    // the global fee growth of the input token
    pub fee_growth_global_x64: u128,
    // amount of input token paid as protocol fee
    pub protocol_fee: u64,
    // the current liquidity in range
    pub liquidity: u128,
}

#[derive(Default)]
struct StepComputations {
    // the price at the beginning of the step
    sqrt_price_start_x64: u128,
    // the next tick to swap to from the current tick in the swap direction
    tick_next: i32,
    // whether tick_next is initialized or not
    initialized: bool,
    // sqrt(price) for the next tick (1/0)
    sqrt_price_next_x64: u128,
    // how much is being swapped in in this step
    amount_in: u64,
    // how much is being swapped out
    amount_out: u64,
    // how much fee is being paid in
    fee_amount: u64,
}

pub fn swap_internal<'b, 'info>(
    ctx: &mut SwapContext<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_specified: i64,
    sqrt_price_limit_x64: u128,
    zero_for_one: bool,
) -> Result<(i64,i64)> {
    require!(amount_specified != 0, ErrorCode::InvaildSwapAmountSpecified);
    require!(
        if zero_for_one {
            sqrt_price_limit_x64 < ctx.pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 > tick_math::MIN_SQRT_RATIO_X64
        } else {
            sqrt_price_limit_x64 > ctx.pool_state.sqrt_price_x64
                && sqrt_price_limit_x64 < tick_math::MAX_SQRT_RATIO_X64
        },
        ErrorCode::SqrtPriceLimitOverflow
    );

    let amm_config = ctx.amm_config.deref();

    let cache = &mut SwapCache {
        liquidity_start: ctx.pool_state.liquidity,
        block_timestamp: oracle::_block_timestamp(),
        protocol_fee_rate: amm_config.protocol_fee_rate,
    };

    let updated_reward_infos = ctx
        .pool_state
        .update_reward_infos(cache.block_timestamp as u64)?;

    let exact_input = amount_specified > 0;

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0,
        sqrt_price_x64: ctx.pool_state.sqrt_price_x64,
        tick: ctx.pool_state.tick_current,
        fee_growth_global_x64: if zero_for_one {
            ctx.pool_state.fee_growth_global_0_x64
        } else {
            ctx.pool_state.fee_growth_global_1_x64
        },
        protocol_fee: 0,
        liquidity: cache.liquidity_start,
    };

    let mut observation_state = ctx.observation_state.load_mut()?;
    let mut remaining_accounts_iter = remaining_accounts.iter();

    let mut tick_array_current_loader = ctx.tick_array_state.load_mut()?;

    // let mut tick_array_loader_next: AccountLoader<TickArrayState>;
    // continue swapping as long as we haven't used the entire input/output and haven't
    // reached the price limit
    while state.amount_specified_remaining != 0 && state.sqrt_price_x64 != sqrt_price_limit_x64 {
        #[cfg(feature = "enable-log")]
        msg!(
            "while begin, exact_input:{},fee_growth_global_x32:{}, state_sqrt_price_x64:{}, state_tick:{},state.protocol_fee:{},cache.protocol_fee_rate:{}",
            exact_input,
            state.fee_growth_global_x64,
            state.sqrt_price_x64,
            state.tick,
            state.protocol_fee,
            cache.protocol_fee_rate
        );
        let mut step = StepComputations::default();
        step.sqrt_price_start_x64 = state.sqrt_price_x64;

        let mut next_initialized_tick = if let Some(tick_state) = tick_array_current_loader.next_initialized_tick(
            state.tick,
            ctx.pool_state.tick_spacing,
            zero_for_one,
        )? {
            Box::new(*tick_state)
        } else {
            Box::new(TickState::default())
        };

        if !next_initialized_tick.is_initialized() {
            let next_array_start_index = tick_array_current_loader
                .next_tick_arrary_start_index(ctx.pool_state.tick_spacing, zero_for_one);

            let tick_array_account_info = remaining_accounts_iter.next().unwrap();
            // ensure this is a valid PDA, even if account is not initialized
            require_keys_eq!(
                tick_array_account_info.key(),
                Pubkey::find_program_address(
                    &[
                        TICK_ARRAY_SEED.as_bytes(),
                        ctx.pool_state.key().as_ref(),
                        &next_array_start_index.to_be_bytes(),
                    ],
                    &crate::id()
                )
                .0
            );

            let tick_array_loader_next =
                AccountLoader::<TickArrayState>::try_from(remaining_accounts_iter.next().unwrap())?;
            let mut tick_array_next = tick_array_loader_next.load_mut()?;
            let mut first_initialized_tick =
                tick_array_next.first_initialized_tick(zero_for_one)?;

            next_initialized_tick = Box::new(*first_initialized_tick.deref_mut());
        }
        step.tick_next = next_initialized_tick.tick;
        step.initialized = next_initialized_tick.is_initialized();

        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        }

        step.sqrt_price_next_x64 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?;
        let target_price = if (zero_for_one && step.sqrt_price_next_x64 < sqrt_price_limit_x64)
            || (!zero_for_one && step.sqrt_price_next_x64 > sqrt_price_limit_x64)
        {
            sqrt_price_limit_x64
        } else {
            step.sqrt_price_next_x64
        };
        let swap_step = swap_math::compute_swap_step(
            state.sqrt_price_x64,
            target_price,
            state.liquidity,
            state.amount_specified_remaining,
            ctx.amm_config.global_fee_rate,
        );
        state.sqrt_price_x64 = swap_step.sqrt_ratio_next_x64;
        step.amount_in = swap_step.amount_in;
        step.amount_out = swap_step.amount_out;
        step.fee_amount = swap_step.fee_amount;

        if exact_input {
            state.amount_specified_remaining -=
                i64::try_from(step.amount_in + step.fee_amount).unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_sub(i64::try_from(step.amount_out).unwrap())
                .unwrap();
        } else {
            state.amount_specified_remaining += i64::try_from(step.amount_out).unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_add(i64::try_from(step.amount_in + step.fee_amount).unwrap())
                .unwrap();
        }

        // if the protocol fee is on, calculate how much is owed, decrement fee_amount, and increment protocol_fee
        if cache.protocol_fee_rate > 0 {
            let delta = step
                .fee_amount
                .checked_mul(cache.protocol_fee_rate as u64)
                .unwrap()
                .checked_div((FEE_RATE_DENOMINATOR_VALUE) as u64)
                .unwrap();
            step.fee_amount -= delta;
            state.protocol_fee += delta;
        }

        // update global fee tracker
        if state.liquidity > 0 {
            // msg!(
            //     "step.fee_amount:{}, state.liquidity:{}",
            //     step.fee_amount,
            //     state.liquidity
            // );
            state.fee_growth_global_x64 += U128::from(step.fee_amount)
                .mul_div_floor(U128::from(fixed_point_64::Q64), U128::from(state.liquidity))
                .unwrap()
                .as_u128();
        }

        // shift tick if we reached the next price
        if state.sqrt_price_x64 == step.sqrt_price_next_x64 {
            // if the tick is initialized, run the tick transition
            if step.initialized {
                #[cfg(feature = "enable-log")]
                msg!("loading next tick {}", step.tick_next);

                let mut liquidity_net = next_initialized_tick.cross(
                    if zero_for_one {
                        state.fee_growth_global_x64
                    } else {
                        ctx.pool_state.fee_growth_global_0_x64
                    },
                    if zero_for_one {
                        ctx.pool_state.fee_growth_global_1_x64
                    } else {
                        state.fee_growth_global_x64
                    },
                    &updated_reward_infos,
                );

                // if we're moving leftward, we interpret liquidity_net as the opposite sign
                // safe because liquidity_net cannot be i64::MIN
                if zero_for_one {
                    liquidity_net = liquidity_net.neg();
                }

                state.liquidity = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
            }

            state.tick = if zero_for_one {
                step.tick_next - 1
            } else {
                step.tick_next
            };
        } else if state.sqrt_price_x64 != step.sqrt_price_start_x64 {
            // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
            state.tick = tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x64)?;
        }

        #[cfg(feature = "enable-log")]
        msg!(
            "end, exact_input:{},step_amount_in:{}, step_amount_out:{}, step_fee_amount:{},fee_growth_global_x32:{}, state_sqrt_price_x64:{}, state_tick:{},state.protocol_fee:{},cache.protocol_fee_rate:{}",
            exact_input,
            step.amount_in,
            step.amount_out,
            step.fee_amount,
            state.fee_growth_global_x64,
            state.sqrt_price_x64,
            state.tick,
            state.protocol_fee,
            cache.protocol_fee_rate
        );
    }

    // update tick
    if state.tick != ctx.pool_state.tick_current {
        ctx.pool_state.tick_current = state.tick;
    }
    // update the previous price to the observation
    let next_observation_index = observation_state.update_check(oracle::_block_timestamp(), ctx.pool_state.sqrt_price_x64, ctx.pool_state.observation_index, ctx.pool_state.observation_update_duration.into()).unwrap();
    match next_observation_index {
        Option::Some(index) => { ctx.pool_state.observation_index = index },
        Option::None => {},
    }
    ctx.pool_state.sqrt_price_x64 = state.sqrt_price_x64;

    // update liquidity if it changed
    if cache.liquidity_start != state.liquidity {
        ctx.pool_state.liquidity = state.liquidity;
    }

    // update fee growth global and, if necessary, protocol fees
    // overflow is acceptable, protocol has to withdraw before it hit u64::MAX fees
    if zero_for_one {
        ctx.pool_state.fee_growth_global_0_x64 = state.fee_growth_global_x64;
        if state.protocol_fee > 0 {
            ctx.pool_state.protocol_fees_token_0 += state.protocol_fee;
        }
    } else {
        ctx.pool_state.fee_growth_global_1_x64 = state.fee_growth_global_x64;
        if state.protocol_fee > 0 {
            ctx.pool_state.protocol_fees_token_1 += state.protocol_fee;
        }
    }

    let (amount_0, amount_1) = if zero_for_one == exact_input {
        (
            amount_specified.saturating_sub(state.amount_specified_remaining),
            state.amount_calculated,
        )
    } else {
        (
            state.amount_calculated,
            amount_specified.saturating_sub(state.amount_specified_remaining),
        )
    };

    Ok((amount_0,amount_1))
}
