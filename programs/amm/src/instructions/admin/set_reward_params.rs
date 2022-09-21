use crate::error::ErrorCode;
use crate::libraries::{fixed_point_64, full_math::MulDiv, U256};
use crate::states::config::AmmConfig;
use crate::states::pool::{reward_period_limit, PoolState, REWARD_NUM};
use crate::util::transfer_from_user_to_pool_vault;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct SetRewardParams<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,

    #[account(
        mut,
        constraint = pool_state.load()?.amm_config == amm_config.key()
    )]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn set_reward_params<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SetRewardParams<'info>>,
    reward_index: u8,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<()> {
    assert!((reward_index as usize) < REWARD_NUM);

    require!(
        ctx.accounts.authority.key() == ctx.accounts.amm_config.owner
            || ctx.accounts.authority.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );

    let current_timestamp = Clock::get()?.unix_timestamp as u64;

    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.update_reward_infos(current_timestamp)?;

    let mut reward_info = pool_state.reward_infos[reward_index as usize];
    if !reward_info.initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }

    if current_timestamp <= reward_info.open_time {
        return err!(ErrorCode::NotApproved);
    }
    let mut reward_amount: u64 = 0;
    if reward_info.last_update_time == reward_info.end_time {
        require_gt!(open_time, current_timestamp);
        require_gt!(emissions_per_second_x64, 0);
        let time_delta = end_time.checked_sub(open_time).unwrap();
        if time_delta < reward_period_limit::MIN_REWARD_PERIOD
            || time_delta > reward_period_limit::MIN_REWARD_PERIOD
        {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }
        reward_amount = U256::from(end_time - open_time)
            .mul_div_ceil(
                U256::from(emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        reward_info.open_time = open_time;
        reward_info.end_time = end_time;
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;
    } else {
        if emissions_per_second_x64 == 0 && end_time == 0 {
            return Err(ErrorCode::InvalidRewardInitParam.into());
        }
        if emissions_per_second_x64 > 0 {
            if reward_info.end_time - current_timestamp > reward_period_limit::INCREASE_EMISSIONES_PERIOD {
                return err!(ErrorCode::NotApproveUpdateRewardEmissiones);
            }
            // emissions_per_second_x64 must not smaller than before
            let emission_diff_x64 = emissions_per_second_x64
                .checked_sub(reward_info.emissions_per_second_x64)
                .unwrap();
            let time_delta = reward_info.end_time - reward_info.last_update_time;
            reward_amount = U256::from(time_delta)
                .mul_div_floor(
                    U256::from(emission_diff_x64),
                    U256::from(fixed_point_64::Q64),
                )
                .unwrap()
                .as_u64();
            reward_info.emissions_per_second_x64 = emissions_per_second_x64;
        }

        if end_time > 0 {
            let time_delta = end_time.checked_sub(reward_info.end_time).unwrap();
            let reward_amount_diff = U256::from(time_delta)
                .mul_div_floor(
                    U256::from(reward_info.emissions_per_second_x64),
                    U256::from(fixed_point_64::Q64),
                )
                .unwrap()
                .as_u64();
            reward_amount = reward_amount.checked_add(reward_amount_diff).unwrap();
            reward_info.end_time = end_time;
        }
    }

    pool_state.reward_infos[reward_index as usize] = reward_info;

    if reward_amount > 0 {
        let mut remaining_accounts = ctx.remaining_accounts.iter();

        let reward_token_vault =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let authority_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let token_program = Program::<Token>::try_from(remaining_accounts.next().unwrap())?;

        require_keys_eq!(reward_token_vault.mint, authority_token_account.mint);
        require_keys_eq!(reward_token_vault.key(), reward_info.token_vault);

        transfer_from_user_to_pool_vault(
            &ctx.accounts.authority,
            &authority_token_account,
            &reward_token_vault,
            &token_program,
            reward_amount,
        )?;
    }

    Ok(())
}
