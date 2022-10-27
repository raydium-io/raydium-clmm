use std::convert::identity;

use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateTickFeeAndRewardGrowth<'info> {
    /// Address to be set as operation account owner.
    #[account(
        mut,
        address = crate::admin::id()
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(mut, constraint = tick_array.load()?.pool_id == pool_state.key())]
    pub tick_array: AccountLoader<'info, TickArrayState>,
}

pub fn update_tick_fee_and_reward_growth_outside(
    ctx: Context<UpdateTickFeeAndRewardGrowth>,
    ticks: Vec<i32>,
) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.load()?;
    let mut tick_array = ctx.accounts.tick_array.load_mut()?;
    for tick in ticks {
        let tick_state = tick_array.get_tick_state_mut(tick, pool_state.tick_spacing as i32)?;

        tick_state.cross(
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            &pool_state.reward_infos,
        );
    }
    Ok(())
}
