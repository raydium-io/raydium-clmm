use crate::error::ErrorCode;
use crate::states::config::AmmConfig;
use crate::states::pool::{PoolState, REWARD_NUM};
use anchor_lang::prelude::*;
use std::ops::DerefMut;

#[derive(Accounts)]
pub struct SetRewardEmissions<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(
        mut
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

pub fn set_reward_emissions<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SetRewardEmissions<'info>>,
    reward_index: u8,
    emissions_per_second_x64: u128,
) -> Result<()> {
    assert!((reward_index as usize) < REWARD_NUM);

    require!(
        ctx.accounts.authority.key() == ctx.accounts.amm_config.owner
            || ctx.accounts.authority.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );

    let clock = Clock::get()?;

    let pool_state = ctx.accounts.pool_state.deref_mut();
    pool_state.update_reward_infos(clock.unix_timestamp as u64)?;

    let reward_info = pool_state.reward_infos[reward_index as usize];

    if !reward_info.initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }

    // if emissions_per_second_x32 > reward_info.emission_per_second_x32 {
    //     let emission_diff = emissions_per_second_x32
    //         .checked_sub(reward_info.emission_per_second_x32)
    //         .unwrap();
    //     let mut remaining_accounts = ctx.remaining_accounts.iter();

    //     let reward_token_vault =
    //         Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
    //     let authority_token_account =
    //         Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
    //     let token_program = Program::<Token>::try_from(remaining_accounts.next().unwrap())?;

    //     require_keys_eq!(reward_token_vault.mint, authority_token_account.mint);
    //     require_keys_eq!(reward_token_vault.key(), reward_info.token_vault);

    //     if pool_state.reward_infos[reward_index as usize].end_time > clock.unix_timestamp as u64 {
    //         let time_delta = pool_state.reward_infos[reward_index as usize]
    //             .end_time
    //             .checked_sub(clock.unix_timestamp as u64)
    //             .unwrap();

    //         let desposit_amount = time_delta
    //             .mul_div_floor(emission_diff, fixed_point_32::Q32)
    //             .unwrap();

    //         transfer_from_user_to_pool_vault(
    //             &ctx.accounts.authority,
    //             &authority_token_account,
    //             &reward_token_vault,
    //             &token_program,
    //             desposit_amount,
    //         )?;
    //     }
    // }

    pool_state.reward_infos[reward_index as usize].emissions_per_second_x64 =
        emissions_per_second_x64;

    Ok(())
}
