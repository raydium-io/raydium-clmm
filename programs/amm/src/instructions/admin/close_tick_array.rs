use crate::states::*;
use crate::util::close_account;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseTickArray<'info> {
    /// Only admin has the authority to reset initial price
    #[account(mut, address = crate::admin::id())]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(mut, constraint = tick_array_state.load()?.pool_id == pool_state.key())]
    pub tick_array_state: AccountLoader<'info, TickArrayState>,
}

pub fn close_tick_array(ctx: Context<CloseTickArray>) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let start_tick_index = ctx.accounts.tick_array_state.load()?.start_tick_index;
    let initialized_tick_count = ctx.accounts.tick_array_state.load()?.initialized_tick_count;
    // close tick_array_lower account
    if initialized_tick_count != 0 {
        pool_state.flip_tick_array_bit(start_tick_index)?;
    }
    close_account(
        &ctx.accounts.tick_array_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
