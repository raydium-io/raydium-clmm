use crate::states::*;
use crate::util::close_account;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ClosePersonalPosition<'info> {
    /// Only admin has the authority to reset initial price
    #[account(address = crate::admin::id())]
    pub owner: Signer<'info>,

    pub pool_state: Box<Account<'info, PoolState>>,

    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,
}

pub fn close_personal_position(ctx: Context<ClosePersonalPosition>) -> Result<()> {
    // close protocol_position account
    close_account(
        &ctx.accounts.personal_position.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
