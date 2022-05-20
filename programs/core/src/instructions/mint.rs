use crate::error::ErrorCode;
use crate::libraries::{liquidity_math, sqrt_price_math, tick_math};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::{Deref, DerefMut};

#[derive(Accounts)]
pub struct MintContext<'info> {
    /// Pays to mint liquidity
    pub minter: Signer<'info>,

    /// The token account spending token_0 to mint the position
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub token_account_0: UncheckedAccount<'info>,

    /// The token account spending token_1 to mint the position
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub token_account_1: UncheckedAccount<'info>,

    /// The address that holds pool tokens for token_0
    #[account(mut)]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(mut)]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// Liquidity is minted on behalf of recipient
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub recipient: UncheckedAccount<'info>,

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The lower tick boundary of the position
    #[account(mut)]
    pub tick_lower_state: AccountLoader<'info, TickState>,

    /// The upper tick boundary of the position
    #[account(mut)]
    pub tick_upper_state: AccountLoader<'info, TickState>,

    /// The bitmap storing initialization state of the lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_lower_state: UncheckedAccount<'info>,

    /// The bitmap storing initialization state of the upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_upper_state: UncheckedAccount<'info>,

    /// The position into which liquidity is minted
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub position_state: UncheckedAccount<'info>,

    /// The program account for the most recent oracle observation, at index = pool.observation_index
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: UncheckedAccount<'info>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,

    /// Program which receives mint_callback
    /// CHECK: Allow arbitrary callback handlers
    pub callback_handler: UncheckedAccount<'info>,
}

pub fn mint<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, MintContext<'info>>,
    amount: u64,
) -> Result<()> {
    let mut pool = ctx.accounts.pool_state.load_mut()?;

    assert!(
        ctx.accounts.vault_0.key()
            == get_associated_token_address(&ctx.accounts.pool_state.key(), &pool.token_0)
    );
    assert!(
        ctx.accounts.vault_1.key()
            == get_associated_token_address(&ctx.accounts.pool_state.key(), &pool.token_1)
    );
    let tick_lower = *ctx.accounts.tick_lower_state.load()?.deref();
    pool.validate_tick_address(
        &ctx.accounts.tick_lower_state.key(),
        tick_lower.bump,
        tick_lower.tick,
    )?;

    let tick_upper = *ctx.accounts.tick_upper_state.load()?.deref();
    pool.validate_tick_address(
        &ctx.accounts.tick_upper_state.key(),
        tick_upper.bump,
        tick_upper.tick,
    )?;

    let bitmap_lower_state = AccountLoader::<TickBitmapState>::try_from(
        &ctx.accounts.bitmap_lower_state.to_account_info(),
    )?;
    pool.validate_bitmap_address(
        &ctx.accounts.bitmap_lower_state.key(),
        bitmap_lower_state.load()?.bump,
        tick_bitmap::position(tick_lower.tick / pool.tick_spacing as i32).word_pos,
    )?;
    let bitmap_upper_state = AccountLoader::<TickBitmapState>::try_from(
        &ctx.accounts.bitmap_upper_state.to_account_info(),
    )?;
    pool.validate_bitmap_address(
        &ctx.accounts.bitmap_upper_state.key(),
        bitmap_upper_state.load()?.bump,
        tick_bitmap::position(tick_upper.tick / pool.tick_spacing as i32).word_pos,
    )?;

    let position_state =
        AccountLoader::<PositionState>::try_from(&ctx.accounts.position_state.to_account_info())?;
    pool.validate_position_address(
        &ctx.accounts.position_state.key(),
        position_state.load()?.bump,
        &ctx.accounts.recipient.key(),
        tick_lower.tick,
        tick_upper.tick,
    )?;

    let last_observation_state = AccountLoader::<ObservationState>::try_from(
        &ctx.accounts.last_observation_state.to_account_info(),
    )?;
    pool.validate_observation_address(
        &last_observation_state.key(),
        last_observation_state.load()?.bump,
        false,
    )?;

    require!(pool.unlocked, ErrorCode::LOK);
    pool.unlocked = false;

    assert!(amount > 0);

    let (amount_0_int, amount_1_int) = _modify_position(
        i64::try_from(amount).unwrap(),
        pool.deref_mut(),
        &position_state,
        &ctx.accounts.tick_lower_state,
        &ctx.accounts.tick_upper_state,
        &bitmap_lower_state,
        &bitmap_upper_state,
        &last_observation_state,
        ctx.remaining_accounts,
    )?;

    let amount_0 = amount_0_int as u64;
    let amount_1 = amount_1_int as u64;

    let balance_0_before = if amount_0 > 0 {
        ctx.accounts.vault_0.amount
    } else {
        0
    };
    let balance_1_before = if amount_1 > 0 {
        ctx.accounts.vault_1.amount
    } else {
        0
    };

    drop(pool);

    if amount_0 > 0 {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_account_0.to_account_info(),
                    to: ctx.accounts.vault_0.to_account_info(),
                    authority: ctx.accounts.minter.to_account_info(),
                },
            ),
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_account_1.to_account_info(),
                    to: ctx.accounts.vault_1.to_account_info(),
                    authority: ctx.accounts.minter.to_account_info(),
                },
            ),
            amount_1,
        )?;
    }

    ctx.accounts.vault_0.reload()?;
    ctx.accounts.vault_1.reload()?;

    if amount_0 > 0 {
        require!(
            balance_0_before + amount_0 <= ctx.accounts.vault_0.amount,
            ErrorCode::M0
        );
    }
    if amount_1 > 0 {
        require!(
            balance_1_before + amount_1 <= ctx.accounts.vault_1.amount,
            ErrorCode::M1
        );
    }

    emit!(MintEvent {
        pool_state: ctx.accounts.pool_state.key(),
        sender: ctx.accounts.minter.key(),
        owner: ctx.accounts.recipient.key(),
        tick_lower: tick_lower.tick,
        tick_upper: tick_upper.tick,
        amount,
        amount_0,
        amount_1
    });

    ctx.accounts.pool_state.load_mut()?.unlocked = true;
    Ok(())
}

/// Credit or debit liquidity to a position, and find the amount of token_0 and token_1
/// required to produce this change.
/// Returns amount of token_0 and token_1 owed to the pool, negative if the pool should
/// pay the recipient.
///
/// # Arguments
///
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Holds the initialization state of the lower tick
/// * `bitmap_upper` - Holds the initialization state of the upper tick
/// * `last_observation_state` - The last written oracle observation, having index = pool.observation_index.
/// This condition must be externally tracked.
/// * `next_observation_state` - The observation account following `last_observation_state`. Becomes equal
/// to last_observation_state when cardinality is 1.
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _modify_position<'info>(
    liquidity_delta: i64,
    pool_state: &mut PoolState,
    position_state: &AccountLoader<'info, PositionState>,
    tick_lower_state: &AccountLoader<'info, TickState>,
    tick_upper_state: &AccountLoader<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
    last_observation_state: &AccountLoader<'info, ObservationState>,
    remaining_accounts: &[AccountInfo<'info>],
) -> Result<(i64, i64)> {
    crate::check_ticks(tick_lower_state.load()?.tick, tick_upper_state.load()?.tick)?;

    let latest_observation = last_observation_state.load()?;

    _update_position(
        liquidity_delta,
        pool_state.deref(),
        latest_observation.deref(),
        position_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower,
        bitmap_upper,
    )?;

    let mut amount_0 = 0;
    let mut amount_1 = 0;

    let tick_lower = tick_lower_state.load()?.tick;
    let tick_upper = tick_upper_state.load()?.tick;

    if liquidity_delta != 0 {
        if pool_state.tick < tick_lower {
            // current tick is below the passed range; liquidity can only become in range by crossing from left to
            // right, when we'll need _more_ token_0 (it's becoming more valuable) so user must provide it
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        } else if pool_state.tick < tick_upper {
            // current tick is inside the passed range
            // write oracle observation
            let timestamp = oracle::_block_timestamp();
            let partition_current_timestamp = timestamp / 14;
            let partition_last_timestamp = latest_observation.block_timestamp / 14;
            drop(latest_observation);

            let next_observation_state;
            let mut new_observation = if partition_current_timestamp > partition_last_timestamp {
                next_observation_state =
                    AccountLoader::<ObservationState>::try_from(&remaining_accounts[0])?;
                let next_observation = next_observation_state.load_mut()?;
                pool_state.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation.bump,
                    true,
                )?;

                next_observation
            } else {
                last_observation_state.load_mut()?
            };

            pool_state.observation_cardinality_next = new_observation.update(
                timestamp,
                pool_state.tick,
                pool_state.liquidity,
                pool_state.observation_cardinality,
                pool_state.observation_cardinality_next,
            );
            pool_state.observation_index = new_observation.index;

            // Both Δtoken_0 and Δtoken_1 will be needed in current price
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                pool_state.sqrt_price_x32,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                pool_state.sqrt_price_x32,
                liquidity_delta,
            );

            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
        // current tick is above the range
        else {
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        }
    }

    Ok((amount_0, amount_1))
}

/// Updates a position with the given liquidity delta
///
/// # Arguments
///
/// * `pool_state` - Current pool state
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Bitmap account for the lower tick
/// * `bitmap_upper` - Bitmap account for the upper tick, if it is different from
/// `bitmap_lower`
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _update_position<'info>(
    liquidity_delta: i64,
    pool_state: &PoolState,
    last_observation_state: &ObservationState,
    position_state: &AccountLoader<'info, PositionState>,
    tick_lower_state: &AccountLoader<'info, TickState>,
    tick_upper_state: &AccountLoader<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
) -> Result<()> {
    let mut tick_lower = tick_lower_state.load_mut()?;
    let mut tick_upper = tick_upper_state.load_mut()?;

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        let time = oracle::_block_timestamp();
        let (tick_cumulative, seconds_per_liquidity_cumulative_x32) =
            last_observation_state.observe_latest(time, pool_state.tick, pool_state.liquidity);

        let max_liquidity_per_tick =
            tick_spacing_to_max_liquidity_per_tick(pool_state.tick_spacing as i32);

        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0_x32,
            pool_state.fee_growth_global_1_x32,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            false,
            max_liquidity_per_tick,
        )?;
        flipped_upper = tick_upper.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0_x32,
            pool_state.fee_growth_global_1_x32,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            true,
            max_liquidity_per_tick,
        )?;

        if flipped_lower {
            let bit_pos = ((tick_lower.tick / pool_state.tick_spacing as i32) % 256) as u8; // rightmost 8 bits
            bitmap_lower.load_mut()?.flip_bit(bit_pos);
        }
        if flipped_upper {
            let bit_pos = ((tick_upper.tick / pool_state.tick_spacing as i32) % 256) as u8;
            if bitmap_lower.key() == bitmap_upper.key() {
                bitmap_lower.load_mut()?.flip_bit(bit_pos);
            } else {
                bitmap_upper.load_mut()?.flip_bit(bit_pos);
            }
        }
    }
    // Update fees accrued to the position
    let (fee_growth_inside_0_x32, fee_growth_inside_1_x32) = tick::get_fee_growth_inside(
        tick_lower.deref(),
        tick_upper.deref(),
        pool_state.tick,
        pool_state.fee_growth_global_0_x32,
        pool_state.fee_growth_global_1_x32,
    );
    position_state.load_mut()?.update(
        liquidity_delta,
        fee_growth_inside_0_x32,
        fee_growth_inside_1_x32,
    )?;

    // Deallocate the tick accounts if they get un-initialized
    // A tick is un-initialized on flip if liquidity_delta is negative
    if liquidity_delta < 0 {
        if flipped_lower {
            tick_lower.clear();
        }
        if flipped_upper {
            tick_upper.clear();
        }
    }
    Ok(())
}
