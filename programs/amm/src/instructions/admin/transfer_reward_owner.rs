use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
#[derive(Accounts)]
pub struct TransferRewardOwner<'info> {
    /// Address to be set as operation account owner.
    #[account(
        address = crate::admin::id() @ ErrorCode::NotApproved
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn transfer_reward_owner<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, TransferRewardOwner<'info>>,
    new_owner: Pubkey,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    for reward_info in &mut pool_state.reward_infos {
        reward_info.authority = new_owner;
    }
    pool_state.owner = new_owner;
    Ok(())
}
