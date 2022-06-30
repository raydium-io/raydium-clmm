use crate::error::ErrorCode;
use crate::libraries::{fixed_point_32, full_math::MulDiv, liquidity_math, swap_math, tick_math};
use crate::states::*;
use crate::util::*;
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

    /// The program account for the most recent oracle observation
    pub last_observation_state: &'b mut Box<Account<'info, ObservationState>>,
}

pub struct SwapCache {
    // the protocol fee for the input token
    pub protocol_fee_rate: u32,
    // liquidity at the beginning of the swap
    pub liquidity_start: u64,
    // the timestamp of the current block
    pub block_timestamp: u32,
    // the current value of the tick accumulator, computed only if we cross an initialized tick
    pub tick_cumulative: i64,
    // the current value of seconds per liquidity accumulator, computed only if we cross an initialized tick
    pub seconds_per_liquidity_cumulative_x32: u64,
    // whether we've computed and cached the above two accumulators
    pub computed_latest_observation: bool,
}

// the top level state of the swap, the results of which are recorded in storage at the end
#[derive(Debug)]
pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    pub amount_specified_remaining: i64,
    // the amount already swapped out/in of the output/input asset
    pub amount_calculated: i64,
    // current sqrt(price)
    pub sqrt_price_x32: u64,
    // the tick associated with the current price
    pub tick: i32,
    // the global fee growth of the input token
    pub fee_growth_global_x32: u64,
    // amount of input token paid as protocol fee
    pub protocol_fee: u64,
    // the current liquidity in range
    pub liquidity: u64,
}

#[derive(Default)]
struct StepComputations {
    // the price at the beginning of the step
    sqrt_price_start_x32: u64,
    // the next tick to swap to from the current tick in the swap direction
    tick_next: i32,
    // whether tick_next is initialized or not
    initialized: bool,
    // sqrt(price) for the next tick (1/0)
    sqrt_price_next_x32: u64,
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
    sqrt_price_limit_x32: u64,
    zero_for_one: bool,
) -> Result<()> {
    require!(amount_specified != 0, ErrorCode::InvaildSwapAmountSpecified);
    require!(
        if zero_for_one {
            sqrt_price_limit_x32 < ctx.pool_state.sqrt_price
                && sqrt_price_limit_x32 > tick_math::MIN_SQRT_RATIO
        } else {
            sqrt_price_limit_x32 > ctx.pool_state.sqrt_price
                && sqrt_price_limit_x32 < tick_math::MAX_SQRT_RATIO
        },
        ErrorCode::SqrtPriceLimitOverflow
    );

    let amm_config = ctx.amm_config.deref();
    let pool_state_info = ctx.pool_state.to_account_info();

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
    assert!(vault_0.key() == ctx.pool_state.token_vault_0);
    assert!(vault_1.key() == ctx.pool_state.token_vault_1);

    ctx.pool_state.validate_observation_address(
        &ctx.last_observation_state.key(),
        ctx.last_observation_state.bump,
        false,
    )?;

    let cache = &mut SwapCache {
        liquidity_start: ctx.pool_state.liquidity,
        block_timestamp: oracle::_block_timestamp(),
        protocol_fee_rate: amm_config.protocol_fee_rate,
        seconds_per_liquidity_cumulative_x32: 0,
        tick_cumulative: 0,
        computed_latest_observation: false,
    };

    let updated_reward_infos = ctx
        .pool_state
        .update_reward_infos(cache.block_timestamp as u64)?;

    let exact_input = amount_specified > 0;

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0,
        sqrt_price_x32: ctx.pool_state.sqrt_price,
        tick: ctx.pool_state.tick,
        fee_growth_global_x32: if zero_for_one {
            ctx.pool_state.fee_growth_global_0
        } else {
            ctx.pool_state.fee_growth_global_1
        },
        protocol_fee: 0,
        liquidity: cache.liquidity_start,
    };

    let latest_observation = ctx.last_observation_state.as_mut();
    let mut remaining_accounts_iter = remaining_accounts.iter();

    // cache for the current bitmap account. Cache is cleared on bitmap transitions
    let mut bitmap_cache: Option<TickBitmapState> = None;

    // continue swapping as long as we haven't used the entire input/output and haven't
    // reached the price limit
    while state.amount_specified_remaining != 0 && state.sqrt_price_x32 != sqrt_price_limit_x32 {
        let mut step = StepComputations::default();
        step.sqrt_price_start_x32 = state.sqrt_price_x32;

        let mut compressed = state.tick / ctx.pool_state.tick_spacing as i32;

        // state.tick is the starting tick for the transition
        if state.tick < 0 && state.tick % ctx.pool_state.tick_spacing as i32 != 0 {
            compressed -= 1; // round towards negative infinity
        }
        // The current tick is not considered in greater than or equal to (lte = false, i.e one for zero) case
        if !zero_for_one {
            compressed += 1;
        }

        let Position { word_pos, bit_pos } = tick_bitmap::position(compressed);

        // load the next bitmap account if cache is empty (first loop instance), or if we have
        // crossed out of this bitmap
        if bitmap_cache.is_none() || bitmap_cache.unwrap().word_pos != word_pos {
            let bitmap_account = remaining_accounts_iter.next().unwrap();
            // ensure this is a valid PDA, even if account is not initialized
            require_keys_eq!(
                bitmap_account.key(),
                Pubkey::find_program_address(
                    &[
                        BITMAP_SEED.as_bytes(),
                        ctx.pool_state.key().as_ref(),
                        &word_pos.to_be_bytes(),
                    ],
                    &crate::id()
                )
                .0
            );

            // read from bitmap if account is initialized, else use default values for next initialized bit
            if let Ok(bitmap_loader) = AccountLoader::<TickBitmapState>::try_from(bitmap_account) {
                let bitmap_state = bitmap_loader.load()?;
                bitmap_cache = Some(*bitmap_state.deref());
            } else {
                // clear cache if the bitmap account was uninitialized. This way default uninitialized
                // values will be returned for the next bit
                msg!("cache cleared");
                bitmap_cache = None;
            }
        }

        // what if bitmap_cache is not updated since next account is not initialized?
        // default values for the next initialized bit if the bitmap account is not initialized
        let next_initialized_bit = if let Some(bitmap) = bitmap_cache {
            bitmap.next_initialized_bit(bit_pos, zero_for_one)
        } else {
            NextBit {
                next: if zero_for_one { 0 } else { 255 },
                initialized: false,
            }
        };

        step.tick_next = (((word_pos as i32) << 8) + next_initialized_bit.next as i32)
            * ctx.pool_state.tick_spacing as i32; // convert relative to absolute
        step.initialized = next_initialized_bit.initialized;

        // ensure that we do not overshoot the min/max tick, as the tick bitmap is not aware of these bounds
        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        }

        step.sqrt_price_next_x32 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?;
        let target_price = if (zero_for_one && step.sqrt_price_next_x32 < sqrt_price_limit_x32)
            || (!zero_for_one && step.sqrt_price_next_x32 > sqrt_price_limit_x32)
        {
            sqrt_price_limit_x32
        } else {
            step.sqrt_price_next_x32
        };
        let swap_step = swap_math::compute_swap_step(
            state.sqrt_price_x32,
            target_price,
            state.liquidity,
            state.amount_specified_remaining,
            ctx.pool_state.fee_rate,
        );
        state.sqrt_price_x32 = swap_step.sqrt_ratio_next_x32;
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
            state.fee_growth_global_x32 += step
                .fee_amount
                .mul_div_floor(fixed_point_32::Q32, state.liquidity)
                .unwrap();
        }
        #[cfg(feature = "enable-log")]
        msg!(
            "exact_input:{},step_amount_in:{}, step_amount_out:{}, step_fee_amount:{},fee_growth_global_x32:{}, state.protocol_fee:{},cache.protocol_fee_rate:{}",
            exact_input,
            step.amount_in,
            step.amount_out,
            step.fee_amount,
            state.fee_growth_global_x32,
            state.protocol_fee,
            cache.protocol_fee_rate
        );
        // shift tick if we reached the next price
        if state.sqrt_price_x32 == step.sqrt_price_next_x32 {
            // if the tick is initialized, run the tick transition
            if step.initialized {
                // check for the placeholder value for the oracle observation, which we replace with the
                // actual value the first time the swap crosses an initialized tick
                if !cache.computed_latest_observation {
                    let new_observation = latest_observation.observe_latest(
                        cache.block_timestamp,
                        ctx.pool_state.tick,
                        ctx.pool_state.liquidity,
                    );
                    cache.tick_cumulative = new_observation.0;
                    cache.seconds_per_liquidity_cumulative_x32 = new_observation.1;
                    cache.computed_latest_observation = true;
                }
                #[cfg(feature = "enable-log")]
                msg!("loading next tick {}", step.tick_next);
                let mut tick_state =
                    Account::<TickState>::try_from(remaining_accounts_iter.next().unwrap())?;
                ctx.pool_state.validate_tick_address(
                    &tick_state.key(),
                    tick_state.bump,
                    step.tick_next,
                )?;
                let mut liquidity_net = tick_state.deref_mut().cross(
                    if zero_for_one {
                        state.fee_growth_global_x32
                    } else {
                        ctx.pool_state.fee_growth_global_0
                    },
                    if zero_for_one {
                        ctx.pool_state.fee_growth_global_1
                    } else {
                        state.fee_growth_global_x32
                    },
                    cache.seconds_per_liquidity_cumulative_x32,
                    cache.tick_cumulative,
                    cache.block_timestamp,
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
        } else if state.sqrt_price_x32 != step.sqrt_price_start_x32 {
            // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
            state.tick = tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x32)?;
        }
    }
    let partition_current_timestamp = cache.block_timestamp / 14;
    let partition_last_timestamp = latest_observation.block_timestamp / 14;

    // update tick and write an oracle entry if the tick changes
    if state.tick != ctx.pool_state.tick {
        // use the next observation account and update pool observation index if block time falls
        // in another partition
        let mut next_observation_state;
        let new_observation = if partition_current_timestamp > partition_last_timestamp {
            next_observation_state =
                Account::<ObservationState>::try_from(remaining_accounts_iter.next().unwrap())?;
            ctx.pool_state.validate_observation_address(
                &next_observation_state.key(),
                next_observation_state.bump,
                true,
            )?;
            next_observation_state.deref_mut()
        } else {
            latest_observation
        };
        ctx.pool_state.tick = state.tick;
        ctx.pool_state.observation_cardinality_next = new_observation.update(
            cache.block_timestamp,
            ctx.pool_state.tick,
            cache.liquidity_start,
            ctx.pool_state.observation_cardinality,
            ctx.pool_state.observation_cardinality_next,
        );
    }
    ctx.pool_state.sqrt_price = state.sqrt_price_x32;

    // update liquidity if it changed
    if cache.liquidity_start != state.liquidity {
        ctx.pool_state.liquidity = state.liquidity;
    }

    // update fee growth global and, if necessary, protocol fees
    // overflow is acceptable, protocol has to withdraw before it hit u64::MAX fees
    if zero_for_one {
        ctx.pool_state.fee_growth_global_0 = state.fee_growth_global_x32;
        if state.protocol_fee > 0 {
            ctx.pool_state.protocol_fees_token_0 += state.protocol_fee;
        }
    } else {
        ctx.pool_state.fee_growth_global_1 = state.fee_growth_global_x32;
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

    if zero_for_one {
        //  x -> y, deposit x token from user to pool vault.
        if amount_0 > 0 {
            transfer_from_user_to_pool_vault(
                &ctx.signer,
                &token_account_0,
                &vault_0,
                &ctx.token_program,
                amount_0 as u64,
            )?;
        }
        // x -> y，transfer y token from pool vault to user.
        if amount_1 < 0 {
            transfer_from_pool_vault_to_user(
                ctx.pool_state,
                &vault_1,
                &token_account_1,
                &ctx.token_program,
                amount_1.neg() as u64,
            )?;
        }
    } else {
        if amount_1 > 0 {
            transfer_from_user_to_pool_vault(
                &ctx.signer,
                &token_account_1,
                &vault_1,
                &ctx.token_program,
                amount_1 as u64,
            )?;
        }
        if amount_0 < 0 {
            transfer_from_pool_vault_to_user(
                ctx.pool_state,
                &vault_0,
                &token_account_0,
                &ctx.token_program,
                amount_0.neg() as u64,
            )?;
        }
    }

    emit!(SwapEvent {
        pool_state: pool_state_info.key(),
        sender: ctx.signer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0,
        amount_1,
        sqrt_price_x32: state.sqrt_price_x32,
        liquidity: state.liquidity,
        tick: state.tick
    });

    Ok(())
}
