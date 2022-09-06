use crate::states::*;
use crate::util::close_account;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ClosePool<'info> {
    /// Only admin has the authority to reset initial price
    #[account(mut, address = crate::admin::id())]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The program account for the oracle observation
    #[account(mut, constraint = observation_state.load()?.amm_pool == pool_state.key())]
    pub observation_state: AccountLoader<'info, ObservationState>,
}

pub fn close_pool(ctx: Context<ClosePool>) -> Result<()> {
    // close pool account
    close_account(
        &ctx.accounts.observation_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;
    close_account(
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
