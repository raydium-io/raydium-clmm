use super::_modify_position;
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::{Deref, DerefMut};

#[derive(Accounts)]
pub struct BurnContext<'info> {
    /// The position owner
    pub owner: Signer<'info>,

    /// Burn liquidity for this pool
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// The lower tick boundary of the position
    /// CHECK: Safety check performed inside function body
    pub tick_lower_state: UncheckedAccount<'info>,

    /// The upper tick boundary of the position
    /// CHECK: Safety check performed inside function body
    pub tick_upper_state: UncheckedAccount<'info>,

    /// The bitmap storing initialization state of the lower tick
    /// CHECK: Safety check performed inside function body
    pub bitmap_lower_state: UncheckedAccount<'info>,

    /// The bitmap storing initialization state of the upper tick
    /// CHECK: Safety check performed inside function body
    pub bitmap_upper_state: UncheckedAccount<'info>,

    /// Burn liquidity from this position
    #[account(mut)]
    pub position_state: AccountLoader<'info, PositionState>,

    /// The program account for the most recent oracle observation
    /// CHECK: Safety check performed inside function body
    pub last_observation_state: UncheckedAccount<'info>,
}

pub fn burn<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, BurnContext<'info>>,
    amount: u64,
) -> Result<()> {
    let pool_state =
        AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
    let mut pool = pool_state.load_mut()?;

    let tick_lower_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
    let tick_lower = *tick_lower_state.load()?.deref();
    pool.validate_tick_address(
        &ctx.accounts.tick_lower_state.key(),
        tick_lower.bump,
        tick_lower.tick,
    )?;

    let tick_upper_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
    let tick_upper = *tick_upper_state.load()?.deref();
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
        &ctx.accounts.owner.key(),
        tick_lower.tick,
        tick_upper.tick,
    )?;

    let last_observation_state = AccountLoader::<ObservationState>::try_from(
        &ctx.accounts.last_observation_state.to_account_info(),
    )?;
    pool.validate_observation_address(
        &ctx.accounts.last_observation_state.key(),
        last_observation_state.load()?.bump,
        false,
    )?;

    msg!("accounts validated");

    require!(pool.unlocked, ErrorCode::LOK);
    pool.unlocked = false;

    let (amount_0_int, amount_1_int) = _modify_position(
        -i64::try_from(amount).unwrap(),
        pool.deref_mut(),
        &ctx.accounts.position_state,
        &tick_lower_state,
        &tick_upper_state,
        &bitmap_lower_state,
        &bitmap_upper_state,
        &last_observation_state,
        ctx.remaining_accounts,
    )?;

    let amount_0 = (-amount_0_int) as u64;
    let amount_1 = (-amount_1_int) as u64;
    if amount_0 > 0 || amount_1 > 0 {
        let mut position_state = ctx.accounts.position_state.load_mut()?;
        position_state.tokens_owed_0 += amount_0;
        position_state.tokens_owed_1 += amount_1;
    }

    emit!(BurnEvent {
        pool_state: ctx.accounts.pool_state.key(),
        owner: ctx.accounts.owner.key(),
        tick_lower: tick_lower.tick,
        tick_upper: tick_lower.tick,
        amount,
        amount_0,
        amount_1,
    });

    pool.unlocked = true;
    Ok(())
}
