use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdatePoolStatus<'info> {
    #[account(
        address = crate::admin::id()
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn update_pool_status(ctx: Context<UpdatePoolStatus>, status: u8) -> Result<()> {
    require_gte!(255, status);
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.set_status(status);
    Ok(())
}
