use crate::states::*;
use anchor_lang::prelude::*;
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct SetRemoveLiquidityTimestamp<'info> {
    #[account(
        address = crate::admin::ID @ ErrorCode::NotApproved
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn set_remove_liquidity_timestamp(ctx: Context<SetRemoveLiquidityTimestamp>) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;

    // Calculate timestamp for 2 days from now (2 days = 172,800 seconds)
    let two_days_in_seconds = 2 * 24 * 60 * 60; // 172,800 seconds
    let new_timestamp = (Clock::get()?.unix_timestamp)
        .checked_add(two_days_in_seconds as i64).unwrap();

    // Set the new timestamp
    pool_state.remove_liquidity_timestamp = new_timestamp;

    // Optional: Emit an event to log the change
    emit!(TimestampUpdatedEvent {
        pool_state: ctx.accounts.pool_state.key(),
        new_timestamp,
    });

    Ok(())
}

