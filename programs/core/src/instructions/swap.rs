use crate::error::ErrorCode;
use crate::libraries::{fixed_point_32, full_math::MulDiv, liquidity_math, swap_math, tick_math};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::Neg;
use std::ops::{Deref, DerefMut};

#[derive(Accounts)]
pub struct SwapContext<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The user token account for input token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub input_token_account: UncheckedAccount<'info>,

    /// The user token account for output token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub output_token_account: UncheckedAccount<'info>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Box<Account<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Box<Account<'info, TokenAccount>>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub factory_state: UncheckedAccount<'info>,

    /// The program account of the pool in which the swap will be performed
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// The program account for the most recent oracle observation
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: UncheckedAccount<'info>,

    /// Program which receives swap_callback
    /// CHECK: Allow arbitrary callback handlers
    pub callback_handler: UncheckedAccount<'info>,
}

pub struct SwapCache {
    // the protocol fee for the input token
    pub fee_protocol: u8,
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

pub fn swap(
    ctx: Context<SwapContext>,
    amount_specified: i64,
    sqrt_price_limit_x32: u64,
) -> Result<()> {
    require!(amount_specified != 0, ErrorCode::AS);

    let factory_state =
        AccountLoader::<FactoryState>::try_from(&ctx.accounts.factory_state.to_account_info())?;

    let pool_loader =
        AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
    let mut pool = pool_loader.load_mut()?;

    let input_token_account = Account::<TokenAccount>::try_from(&ctx.accounts.input_token_account)?;
    let output_token_account =
        Account::<TokenAccount>::try_from(&ctx.accounts.output_token_account)?;

    let zero_for_one = ctx.accounts.input_vault.mint == pool.token_0;

    let (token_account_0, token_account_1, mut vault_0, mut vault_1) = if zero_for_one {
        (
            input_token_account,
            output_token_account,
            ctx.accounts.input_vault.clone(),
            ctx.accounts.output_vault.clone(),
        )
    } else {
        (
            output_token_account,
            input_token_account,
            ctx.accounts.output_vault.clone(),
            ctx.accounts.input_vault.clone(),
        )
    };
    assert!(vault_0.key() == get_associated_token_address(&pool_loader.key(), &pool.token_0));
    assert!(vault_1.key() == get_associated_token_address(&pool_loader.key(), &pool.token_1));

    let last_observation_state = AccountLoader::<ObservationState>::try_from(
        &ctx.accounts.last_observation_state.to_account_info(),
    )?;
    pool.validate_observation_address(
        &ctx.accounts.last_observation_state.key(),
        last_observation_state.load()?.bump,
        false,
    )?;

    require!(pool.unlocked, ErrorCode::LOK);
    require!(
        if zero_for_one {
            sqrt_price_limit_x32 < pool.sqrt_price_x32
                && sqrt_price_limit_x32 > tick_math::MIN_SQRT_RATIO
        } else {
            sqrt_price_limit_x32 > pool.sqrt_price_x32
                && sqrt_price_limit_x32 < tick_math::MAX_SQRT_RATIO
        },
        ErrorCode::SPL
    );

    pool.unlocked = false;
    let mut cache = SwapCache {
        liquidity_start: pool.liquidity,
        block_timestamp: oracle::_block_timestamp(),
        fee_protocol: factory_state.load()?.fee_protocol,
        seconds_per_liquidity_cumulative_x32: 0,
        tick_cumulative: 0,
        computed_latest_observation: false,
    };

    let exact_input = amount_specified > 0;

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0,
        sqrt_price_x32: pool.sqrt_price_x32,
        tick: pool.tick,
        fee_growth_global_x32: if zero_for_one {
            pool.fee_growth_global_0_x32
        } else {
            pool.fee_growth_global_1_x32
        },
        protocol_fee: 0,
        liquidity: cache.liquidity_start,
    };

    let latest_observation = last_observation_state.load_mut()?;
    let mut remaining_accounts = ctx.remaining_accounts.iter();

    // cache for the current bitmap account. Cache is cleared on bitmap transitions
    let mut bitmap_cache: Option<TickBitmapState> = None;

    // continue swapping as long as we haven't used the entire input/output and haven't
    // reached the price limit
    while state.amount_specified_remaining != 0 && state.sqrt_price_x32 != sqrt_price_limit_x32 {
        let mut step = StepComputations::default();
        step.sqrt_price_start_x32 = state.sqrt_price_x32;

        let mut compressed = state.tick / pool.tick_spacing as i32;

        // state.tick is the starting tick for the transition
        if state.tick < 0 && state.tick % pool.tick_spacing as i32 != 0 {
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
            let bitmap_account = remaining_accounts.next().unwrap();
            msg!("check bitmap {}", word_pos);
            // ensure this is a valid PDA, even if account is not initialized
            assert!(
                bitmap_account.key()
                    == Pubkey::find_program_address(
                        &[
                            BITMAP_SEED.as_bytes(),
                            pool.token_0.as_ref(),
                            pool.token_1.as_ref(),
                            &pool.fee.to_be_bytes(),
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
            * pool.tick_spacing as i32; // convert relative to absolute
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
            pool.fee,
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
        if cache.fee_protocol > 0 {
            let delta = step.fee_amount / cache.fee_protocol as u64;
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

        // shift tick if we reached the next price
        if state.sqrt_price_x32 == step.sqrt_price_next_x32 {
            // if the tick is initialized, run the tick transition
            if step.initialized {
                // check for the placeholder value for the oracle observation, which we replace with the
                // actual value the first time the swap crosses an initialized tick
                if !cache.computed_latest_observation {
                    let new_observation = latest_observation.observe_latest(
                        cache.block_timestamp,
                        pool.tick,
                        pool.liquidity,
                    );
                    cache.tick_cumulative = new_observation.0;
                    cache.seconds_per_liquidity_cumulative_x32 = new_observation.1;
                    cache.computed_latest_observation = true;
                }

                msg!("loading tick {}", step.tick_next);
                let tick_loader =
                    AccountLoader::<TickState>::try_from(remaining_accounts.next().unwrap())?;
                let mut tick_state = tick_loader.load_mut()?;
                pool.validate_tick_address(&tick_loader.key(), tick_state.bump, step.tick_next)?;
                let mut liquidity_net = tick_state.deref_mut().cross(
                    if zero_for_one {
                        state.fee_growth_global_x32
                    } else {
                        pool.fee_growth_global_0_x32
                    },
                    if zero_for_one {
                        pool.fee_growth_global_1_x32
                    } else {
                        state.fee_growth_global_x32
                    },
                    cache.seconds_per_liquidity_cumulative_x32,
                    cache.tick_cumulative,
                    cache.block_timestamp,
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
    drop(latest_observation);

    // update tick and write an oracle entry if the tick changes
    if state.tick != pool.tick {
        // use the next observation account and update pool observation index if block time falls
        // in another partition
        let next_observation_state;
        let mut next_observation = if partition_current_timestamp > partition_last_timestamp {
            next_observation_state =
                AccountLoader::<ObservationState>::try_from(&remaining_accounts.next().unwrap())?;
            let next_observation = next_observation_state.load_mut()?;

            pool.validate_observation_address(
                &next_observation_state.key(),
                next_observation.bump,
                true,
            )?;

            next_observation
        } else {
            last_observation_state.load_mut()?
        };
        pool.tick = state.tick;
        pool.observation_cardinality_next = next_observation.update(
            cache.block_timestamp,
            pool.tick,
            cache.liquidity_start,
            pool.observation_cardinality,
            pool.observation_cardinality_next,
        );
    }
    pool.sqrt_price_x32 = state.sqrt_price_x32;

    // update liquidity if it changed
    if cache.liquidity_start != state.liquidity {
        pool.liquidity = state.liquidity;
    }

    // update fee growth global and, if necessary, protocol fees
    // overflow is acceptable, protocol has to withdraw before it hit u64::MAX fees
    if zero_for_one {
        pool.fee_growth_global_0_x32 = state.fee_growth_global_x32;
        if state.protocol_fee > 0 {
            pool.protocol_fees_token_0 += state.protocol_fee;
        }
    } else {
        pool.fee_growth_global_1_x32 = state.fee_growth_global_x32;
        if state.protocol_fee > 0 {
            pool.protocol_fees_token_1 += state.protocol_fee;
        }
    }

    let (amount_0, amount_1) = if zero_for_one == exact_input {
        (
            amount_specified - state.amount_specified_remaining,
            state.amount_calculated,
        )
    } else {
        (
            state.amount_calculated,
            amount_specified - state.amount_specified_remaining,
        )
    };

    // do the transfers and collect payment
    let pool_state_seeds = [
        &POOL_SEED.as_bytes(),
        &pool.token_0.to_bytes() as &[u8],
        &pool.token_1.to_bytes() as &[u8],
        &pool.fee.to_be_bytes(),
        &[pool.bump],
    ];
    drop(pool);

    msg!("vault balances {} {}", vault_0.amount, vault_1.amount);

    if zero_for_one {
        // x -> y，transfer y token from pool vault to user.
        if amount_1 < 0 {
            msg!("paying {}", amount_1.neg());
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: vault_1.to_account_info().clone(),
                        to: token_account_1.to_account_info().clone(),
                        authority: ctx.accounts.pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_1.neg() as u64,
            )?;
        }
        let balance_0_before = vault_0.amount;
        //  x -> y, deposit x token from user to pool vault.
        if amount_0 > 0 {
            msg!(
                "amount to pay {}, delta 0 {}, delta 1 {}",
                amount_0 as u64,
                amount_0,
                amount_1
            );
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    token::Transfer {
                        from: token_account_0.to_account_info(),
                        to: vault_0.to_account_info(),
                        authority: ctx.accounts.signer.to_account_info(),
                    },
                ),
                amount_0 as u64,
            )?;
        }

        vault_0.reload()?;
        require!(
            balance_0_before.checked_add(amount_0 as u64).unwrap() <= vault_0.amount,
            ErrorCode::IIA
        );
    } else {
        if amount_0 < 0 {
            msg!("paying {}", amount_0.neg());
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: vault_0.to_account_info().clone(),
                        to: token_account_0.to_account_info().clone(),
                        authority: ctx.accounts.pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_0.neg() as u64,
            )?;
        }
        let balance_1_before = vault_1.amount;

        if amount_1 > 0 {
            msg!(
                "amount to pay {}, delta 0 {}, delta 1 {}",
                amount_1 as u64,
                amount_0,
                amount_1
            );
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    token::Transfer {
                        from: token_account_1.to_account_info(),
                        to: vault_1.to_account_info(),
                        authority: ctx.accounts.signer.to_account_info(),
                    },
                ),
                amount_1 as u64,
            )?;
        }

        vault_1.reload()?;
        require!(
            balance_1_before.checked_add(amount_1 as u64).unwrap() <= vault_1.amount,
            ErrorCode::IIA
        );
    }

    emit!(SwapEvent {
        pool_state: pool_loader.key(),
        sender: ctx.accounts.signer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0,
        amount_1,
        sqrt_price_x32: state.sqrt_price_x32,
        liquidity: state.liquidity,
        tick: state.tick
    });
    pool_loader.load_mut()?.unlocked = true;

    Ok(())
}
