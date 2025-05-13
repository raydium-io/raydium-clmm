use crate::error::ErrorCode;
use crate::libraries::{fixed_point_64, full_math::MulDiv, U256};
use crate::states::pool::{reward_period_limit, PoolState, REWARD_NUM};
use crate::util::transfer_from_user_to_pool_vault;
use crate::{states::*, util};
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct SetRewardParams<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    pub authority: Signer<'info>,

    #[account(
        address = pool_state.load()?.amm_config
    )]
    pub amm_config: Account<'info, AmmConfig>,

    #[account(
        mut,
        constraint = pool_state.load()?.amm_config == amm_config.key()
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// load info from the account to judge reward permission
    #[account(
        seeds = [
            OPERATION_SEED.as_bytes(),
        ],
        bump,
    )]
    pub operation_state: AccountLoader<'info, OperationState>,

    /// Token program
    pub token_program: Program<'info, Token>,
    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,
}

pub fn set_reward_params<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SetRewardParams<'info>>,
    reward_index: u8,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<()> {
    assert!((reward_index as usize) < REWARD_NUM);
    require_gt!(end_time, open_time);
    require_gt!(emissions_per_second_x64, 0);
    let operation_state = ctx.accounts.operation_state.load()?;
    let admin_keys = operation_state.operation_owners.to_vec();
    let admin_operator = admin_keys.contains(&ctx.accounts.authority.key())
        && ctx.accounts.authority.key() != Pubkey::default();

    let current_timestamp = u64::try_from(Clock::get()?.unix_timestamp).unwrap();
    require_gt!(open_time, current_timestamp);

    let mut pool_state = ctx.accounts.pool_state.load_mut()?;

    if !admin_operator {
        require_keys_eq!(ctx.accounts.authority.key(), pool_state.owner);
    }

    pool_state.update_reward_infos(current_timestamp)?;

    let mut reward_info = pool_state.reward_infos[reward_index as usize];
    if !reward_info.initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }

    let reward_amount = if admin_operator {
        admin_update(
            &mut reward_info,
            current_timestamp,
            emissions_per_second_x64,
            open_time,
            end_time,
        )
        .unwrap()
    } else {
        if current_timestamp <= reward_info.open_time {
            return err!(ErrorCode::NotApproved);
        }
        normal_update(
            &mut reward_info,
            current_timestamp,
            emissions_per_second_x64,
            open_time,
            end_time,
        )
        .unwrap()
    };

    pool_state.reward_infos[reward_index as usize] = reward_info;

    if reward_amount > 0 {
        let mut remaining_accounts = ctx.remaining_accounts.iter();

        let reward_token_vault =
            InterfaceAccount::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let authority_token_account =
            InterfaceAccount::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let reward_vault_mint =
            InterfaceAccount::<Mint>::try_from(&remaining_accounts.next().unwrap())?;

        require_keys_eq!(reward_token_vault.mint, authority_token_account.mint);
        require_keys_eq!(reward_token_vault.key(), reward_info.token_vault);

        let transfer_fee: u64 =
            util::get_transfer_inverse_fee(Box::new(reward_vault_mint.clone()), reward_amount)
                .unwrap();
        let reward_amount_with_transfer_fee = reward_amount.checked_add(transfer_fee).unwrap();

        transfer_from_user_to_pool_vault(
            &ctx.accounts.authority,
            &authority_token_account.to_account_info(),
            &reward_token_vault.to_account_info(),
            Some(Box::new(reward_vault_mint)),
            &ctx.accounts.token_program,
            Some(ctx.accounts.token_program_2022.to_account_info()),
            reward_amount_with_transfer_fee,
        )?;
    }

    Ok(())
}

fn normal_update(
    reward_info: &mut RewardInfo,
    current_timestamp: u64,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<u64> {
    let mut reward_amount: u64;
    if reward_info.last_update_time == reward_info.end_time {
        // reward emission has finished
        let time_delta = end_time.checked_sub(open_time).unwrap();
        if time_delta < reward_period_limit::MIN_REWARD_PERIOD
            || time_delta > reward_period_limit::MAX_REWARD_PERIOD
        {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }
        reward_amount = U256::from(time_delta)
            .mul_div_ceil(
                U256::from(emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        reward_info.open_time = open_time;
        reward_info.last_update_time = open_time;
        reward_info.end_time = end_time;
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;
    } else {
        // reward emission does not finish
        let left_reward_time = reward_info.end_time.checked_sub(current_timestamp).unwrap();
        let extend_period = end_time.checked_sub(reward_info.end_time).unwrap();
        if extend_period < reward_period_limit::MIN_REWARD_PERIOD
            || extend_period > reward_period_limit::MAX_REWARD_PERIOD
        {
            return err!(ErrorCode::NotApproveUpdateRewardEmissiones);
        }

        // emissions_per_second_x64 must not smaller than before with in 72hrs
        if emissions_per_second_x64 < reward_info.emissions_per_second_x64 {
            require_gt!(
                reward_period_limit::INCREASE_EMISSIONES_PERIOD,
                left_reward_time
            );
        }
        let emission_diff_x64 =
            emissions_per_second_x64.saturating_sub(reward_info.emissions_per_second_x64);
        reward_amount = U256::from(left_reward_time)
            .mul_div_floor(
                U256::from(emission_diff_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;

        if extend_period > 0 {
            let reward_amount_diff = U256::from(extend_period)
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

    Ok(reward_amount)
}

fn admin_update(
    reward_info: &mut RewardInfo,
    current_timestamp: u64,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
) -> Result<u64> {
    let mut reward_amount: u64;
    if reward_info.last_update_time == reward_info.end_time
        || reward_info.open_time > current_timestamp
    {
        // reward emission has finished
        let time_delta = end_time.checked_sub(open_time).unwrap();
        if time_delta == 0 {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }
        reward_amount = U256::from(time_delta)
            .mul_div_ceil(
                U256::from(emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();

        reward_info.open_time = open_time;
        reward_info.last_update_time = open_time;
        reward_info.end_time = end_time;
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;
    } else {
        // reward emission does not finish
        let left_reward_time = reward_info.end_time.checked_sub(current_timestamp).unwrap();
        let extend_period = end_time.saturating_sub(reward_info.end_time);

        // emissions_per_second_x64 can be update for admin during anytime
        let emission_diff_x64 =
            emissions_per_second_x64.saturating_sub(reward_info.emissions_per_second_x64);
        reward_amount = U256::from(left_reward_time)
            .mul_div_floor(
                U256::from(emission_diff_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();
        reward_info.emissions_per_second_x64 = emissions_per_second_x64;

        let reward_amount_diff = U256::from(extend_period)
            .mul_div_floor(
                U256::from(reward_info.emissions_per_second_x64),
                U256::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64();
        reward_amount = reward_amount.checked_add(reward_amount_diff).unwrap();
        reward_info.end_time = end_time;
    }

    Ok(reward_amount)
}
