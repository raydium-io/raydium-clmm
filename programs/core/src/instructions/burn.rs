use super::_modify_position;
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct BurnContext<'info> {
    /// The position owner
    pub owner: Signer<'info>,

    /// Burn liquidity for this pool
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The lower tick boundary of the position
    /// CHECK: Safety check performed inside function body
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The upper tick boundary of the position
    /// CHECK: Safety check performed inside function body
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The bitmap storing initialization state of the lower tick
    /// CHECK: Safety check performed inside function body
    pub bitmap_lower_state: Box<Account<'info, TickBitmapState>>,

    /// The bitmap storing initialization state of the upper tick
    /// CHECK: Safety check performed inside function body
    pub bitmap_upper_state: Box<Account<'info, TickBitmapState>>,

    /// Burn liquidity from this position
    #[account(mut)]
    pub position_state: Box<Account<'info, PositionState>>,

    /// The program account for the most recent oracle observation
    /// CHECK: Safety check performed inside function body
    pub last_observation_state: Box<Account<'info, ObservationState>>,
}

pub fn burn<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, BurnContext<'info>>,
    amount: u64,
) -> Result<()> {
    let pool_state_info = ctx.accounts.pool_state.to_account_info();
    let pool_state = ctx.accounts.pool_state.as_mut();

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
        &ctx.accounts.owner.key(),
        ctx.accounts.tick_lower_state.tick,
        ctx.accounts.tick_upper_state.tick,
    )?;

    pool_state.validate_observation_address(
        &ctx.accounts.last_observation_state.key(),
        ctx.accounts.last_observation_state.bump,
        false,
    )?;

    msg!("accounts validated");

    require!(pool_state.unlocked, ErrorCode::LOK);
    pool_state.unlocked = false;

    let (amount_0_int, amount_1_int) = _modify_position(
        -i64::try_from(amount).unwrap(),
        pool_state,
        ctx.accounts.position_state.as_mut(),
        ctx.accounts.tick_lower_state.as_mut(),
        ctx.accounts.tick_upper_state.as_mut(),
        ctx.accounts.bitmap_lower_state.as_mut(),
        ctx.accounts.bitmap_upper_state.as_mut(),
        ctx.accounts.last_observation_state.as_mut(),
        ctx.remaining_accounts,
    )?;

    let amount_0 = (-amount_0_int) as u64;
    let amount_1 = (-amount_1_int) as u64;
    if amount_0 > 0 || amount_1 > 0 {
        let position_state = &mut ctx.accounts.position_state;
        position_state.tokens_owed_0 += amount_0;
        position_state.tokens_owed_1 += amount_1;
    }

    emit!(BurnEvent {
        pool_state: pool_state_info.key(),
        owner: ctx.accounts.owner.key(),
        tick_lower: ctx.accounts.tick_lower_state.tick,
        tick_upper: ctx.accounts.tick_upper_state.tick,
        amount,
        amount_0,
        amount_1,
    });

    pool_state.unlocked = true;
    Ok(())
}
