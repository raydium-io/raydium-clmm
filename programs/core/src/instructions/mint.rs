use crate::error::ErrorCode;
use crate::libraries::{liquidity_math, sqrt_price_math, tick_math};
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::{Deref, DerefMut};

#[derive(Accounts)]
pub struct MintContext<'info> {
    /// Pays to mint liquidity
    pub minter: Signer<'info>,

    /// The token account spending token_0 to mint the position
    #[account(
        mut,
        token::mint = vault_0.mint
    )]
    pub token_account_0: Box<Account<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
    #[account(
        mut,
        token::mint = vault_1.mint
    )]
    pub token_account_1: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = vault_0.key() == pool_state.token_vault_0
    )]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = vault_1.key() == pool_state.token_vault_1
    )]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// Liquidity is minted on behalf of recipient
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub recipient: UncheckedAccount<'info>,

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The lower tick boundary of the position
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The upper tick boundary of the position
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The bitmap storing initialization state of the lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_lower_state: Box<Account<'info, TickBitmapState>>,

    /// The bitmap storing initialization state of the upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_upper_state: Box<Account<'info, TickBitmapState>>,

    /// The position into which liquidity is minted
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub position_state: Box<Account<'info, PositionState>>,

    /// The program account for the most recent oracle observation, at index = pool.observation_index
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

pub fn mint<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, MintContext<'info>>,
    amount: u64,
) -> Result<()> {
    let pool_state_info = ctx.accounts.pool_state.to_account_info();
    let pool_state = &mut ctx.accounts.pool_state;

    assert!(ctx.accounts.vault_0.key() == pool_state.token_vault_0);
    assert!(ctx.accounts.vault_1.key() == pool_state.token_vault_1);
    pool_state.validate_tick_address(
        &ctx.accounts.tick_lower_state.key(),
        ctx.accounts.tick_lower_state.bump,
        ctx.accounts.tick_lower_state.tick,
    )?;
    pool_state.validate_tick_address(
        &ctx.accounts.tick_upper_state.key(),
        ctx.accounts.tick_upper_state.bump,
        ctx.accounts.tick_upper_state.tick,
    )?;
    pool_state.validate_bitmap_address(
        &ctx.accounts.bitmap_lower_state.key(),
        ctx.accounts.bitmap_lower_state.bump,
        tick_bitmap::position(ctx.accounts.tick_lower_state.tick / pool_state.tick_spacing as i32)
            .word_pos,
    )?;
    pool_state.validate_bitmap_address(
        &ctx.accounts.bitmap_upper_state.key(),
        ctx.accounts.bitmap_upper_state.bump,
        tick_bitmap::position(ctx.accounts.tick_upper_state.tick / pool_state.tick_spacing as i32)
            .word_pos,
    )?;

    pool_state.validate_position_address(
        &ctx.accounts.position_state.key(),
        ctx.accounts.position_state.bump,
        &ctx.accounts.recipient.key(),
        ctx.accounts.tick_lower_state.tick,
        ctx.accounts.tick_upper_state.tick,
    )?;
    pool_state.validate_observation_address(
        &ctx.accounts.last_observation_state.key(),
        ctx.accounts.last_observation_state.bump,
        false,
    )?;

    require!(pool_state.unlocked, ErrorCode::LOK);
    pool_state.unlocked = false;

    assert!(amount > 0);

    let (amount_0_int, amount_1_int) = _modify_position(
        i64::try_from(amount).unwrap(),
        pool_state,
        ctx.accounts.position_state.as_mut(),
        ctx.accounts.tick_lower_state.as_mut(),
        ctx.accounts.tick_upper_state.as_mut(),
        ctx.accounts.bitmap_lower_state.as_mut(),
        ctx.accounts.bitmap_upper_state.as_mut(),
        ctx.accounts.last_observation_state.as_mut(),
        ctx.remaining_accounts,
    )?;

    let amount_0 = amount_0_int as u64;
    let amount_1 = amount_1_int as u64;

    if amount_0 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.accounts.minter,
            &ctx.accounts.token_account_0,
            &ctx.accounts.vault_0,
            &ctx.accounts.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.accounts.minter,
            &ctx.accounts.token_account_1,
            &ctx.accounts.vault_1,
            &ctx.accounts.token_program,
            amount_1,
        )?;
    }
    emit!(MintEvent {
        pool_state: pool_state_info.key(),
        sender: ctx.accounts.minter.key(),
        owner: ctx.accounts.recipient.key(),
        tick_lower: ctx.accounts.tick_lower_state.tick,
        tick_upper: ctx.accounts.tick_upper_state.tick,
        amount,
        amount_0,
        amount_1
    });

    pool_state.unlocked = true;
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
    pool_state: &mut Account<'info, PoolState>,
    position_state: &mut Account<'info, PositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &mut Account<'info, TickBitmapState>,
    bitmap_upper: &mut Account<'info, TickBitmapState>,
    last_observation_state: &mut Account<'info, ObservationState>,
    remaining_accounts: &[AccountInfo<'info>],
) -> Result<(i64, i64)> {
    crate::check_ticks(tick_lower_state.tick, tick_upper_state.tick)?;

    _update_position(
        liquidity_delta,
        pool_state,
        last_observation_state,
        position_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower,
        bitmap_upper,
    )?;

    let mut amount_0 = 0;
    let mut amount_1 = 0;

    let tick_lower = tick_lower_state.tick;
    let tick_upper = tick_upper_state.tick;

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
            let partition_last_timestamp = last_observation_state.block_timestamp / 14;

            let mut next_observation_state;
            let new_observation = if partition_current_timestamp > partition_last_timestamp {
                next_observation_state =
                    Account::<ObservationState>::try_from(&remaining_accounts[0])?;
                // let next_observation = next_observation_state.deref_mut();
                pool_state.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation_state.bump,
                    true,
                )?;

                next_observation_state.deref_mut()
            } else {
                last_observation_state.deref_mut()
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
    pool_state: &mut Account<'info, PoolState>,
    last_observation_state: &mut Account<ObservationState>,
    position_state: &mut Account<'info, PositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &mut Account<'info, TickBitmapState>,
    bitmap_upper: &mut Account<'info, TickBitmapState>,
) -> Result<()> {
    let tick_lower = tick_lower_state.deref_mut();
    let tick_upper = tick_upper_state.deref_mut();

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
            bitmap_lower.flip_bit(bit_pos);
        }
        if flipped_upper {
            let bit_pos = ((tick_upper.tick / pool_state.tick_spacing as i32) % 256) as u8;
            if bitmap_lower.key() == bitmap_upper.key() {
                bitmap_lower.flip_bit(bit_pos);
            } else {
                bitmap_upper.flip_bit(bit_pos);
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
    position_state.update(
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
