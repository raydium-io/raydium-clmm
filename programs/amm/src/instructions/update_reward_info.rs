use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateRewardInfos<'info> {
    /// tThe liquidity pool for which reward info to update
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,
}

pub fn update_reward_infos<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, UpdateRewardInfos<'info>>,
) -> Result<()> {
    let clock = Clock::get()?;
    let pool_state = ctx.accounts.pool_state.as_mut();
    let updated_reward_infos = pool_state.update_reward_infos(clock.unix_timestamp as u64)?;

    emit!(UpdateRewardInfosEvent {
        reward_infos: updated_reward_infos
    });

    Ok(())
}
