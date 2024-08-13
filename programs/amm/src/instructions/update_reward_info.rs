use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateRewardInfos<'info> {
    /// The liquidity pool for which reward info to update
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn update_reward_infos<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, UpdateRewardInfos<'info>>,
) -> Result<()> {
    let clock = Clock::get()?;
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let updated_reward_infos =
        pool_state.update_reward_infos(u64::try_from(clock.unix_timestamp).unwrap())?;

    emit!(UpdateRewardInfosEvent {
        reward_growth_global_x64: RewardInfo::get_reward_growths(&updated_reward_infos)
    });

    Ok(())
}
