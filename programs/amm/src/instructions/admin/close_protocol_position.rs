use crate::states::*;
use crate::util::close_account;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseProtocolPosition<'info> {
    /// Only admin has the authority to reset initial price
    #[account(mut, address = crate::admin::id())]
    pub owner: Signer<'info>,

    #[account()]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(mut, constraint = protocol_position_state.pool_id == pool_state.key())]
    pub protocol_position_state: Box<Account<'info, ProtocolPositionState>>,
}

pub fn close_protocol_position(ctx: Context<CloseProtocolPosition>) -> Result<()> {
    // close protocol_position account
    close_account(
        &ctx.accounts.protocol_position_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
