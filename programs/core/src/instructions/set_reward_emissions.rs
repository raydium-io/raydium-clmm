use std::ops::DerefMut;

use crate::error::ErrorCode;
use crate::states::config::AmmConfig;
use crate::states::pool::PoolState;
use anchor_lang::prelude::*;
#[derive(Accounts)]
pub struct SetRewardEmissions<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(
        mut,
        address = amm_config.owner.key()
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,

    #[account(
        mut,
        constraint = pool_state.amm_config == amm_config.key()
    )]
    pub pool_state: Box<Account<'info, PoolState>>,
}

pub fn set_reward_emissions(
    ctx: Context<SetRewardEmissions>,
    reward_index: u8,
    emissions_per_second_x32: u64,
) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.deref_mut();
    let clock = Clock::get()?;
    pool_state.update_reward_infos(clock.unix_timestamp as u64)?;
    if !pool_state.reward_infos[reward_index as usize].initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }

    pool_state.reward_infos[reward_index as usize].reward_emission_per_second_x32 =
        emissions_per_second_x32;

    Ok(())
}
