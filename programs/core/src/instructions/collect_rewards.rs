use std::ops::Deref;

use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct CollectRewards<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub protocol_position_state: Box<Account<'info, ProcotolPositionState>>,

    /// The program account of the NFT for which tokens are being collected
    #[account(mut)]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

    /// The program account for the liquidity pool from which fees are collected
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Account to store data for the position's lower tick
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn collect_rewards<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CollectRewards<'info>>,
) -> Result<()> {
    let remaining_accounts_len = ctx.remaining_accounts.len();
    if remaining_accounts_len < 2
        || remaining_accounts_len % 2 != 0
        || remaining_accounts_len > NUM_REWARDS * 2
    {
        return err!(ErrorCode::InvalidRewardInputAccountNumber);
    }

    let clock = Clock::get()?;
    let pool_state = ctx.accounts.pool_state.as_mut();
    // update global reward info
    let updated_reward_infos = pool_state.update_reward_infos(clock.unix_timestamp as u64)?;
    let reward_growths_inside = get_updated_reward_growths_inside(
        &mut &mut ctx.accounts.protocol_position_state,
        &ctx.accounts.tick_lower_state,
        &ctx.accounts.tick_upper_state,
        pool_state.tick,
        &updated_reward_infos,
    );
    let tokenized_position = &mut ctx.accounts.personal_position_state;
    tokenized_position.update_rewards(reward_growths_inside)?;

    let mut remaining_accounts = ctx.remaining_accounts.iter();
    for i in 0..remaining_accounts_len / 2 {
        let reward_token_vault =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let recipient_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        require_keys_eq!(reward_token_vault.mint, recipient_token_account.mint);
        require_keys_eq!(
            reward_token_vault.key(),
            pool_state.reward_infos[i].reward_token_vault
        );

        let reward_amount_owed = tokenized_position.reward_infos[i].reward_amount_owed;
        if reward_amount_owed == 0 {
            continue;
        }

        let transfer_amount = if reward_amount_owed > reward_token_vault.amount {
            reward_token_vault.amount
        } else {
            reward_amount_owed
        };

        if transfer_amount > 0 {
            msg!(
                "collect reward index: {}, transfer_amount: {}, reward_amount_owed:{} ",
                i,
                transfer_amount,
                reward_amount_owed
            );
            tokenized_position.reward_infos[i].reward_amount_owed =
                reward_amount_owed.checked_sub(transfer_amount).unwrap();

            transfer_from_pool_vault_to_user(
                pool_state,
                &reward_token_vault,
                &recipient_token_account,
                &ctx.accounts.token_program,
                transfer_amount,
            )?;

            pool_state.add_reward_clamed(i, transfer_amount)?;
        }
    }

    Ok(())
}

fn get_updated_reward_growths_inside<'info>(
    procotol_position_state: &mut Account<'info, ProcotolPositionState>,
    tick_lower_state: &Account<'info, TickState>,
    tick_upper_state: &Account<'info, TickState>,
    current_tick: i32,
    updated_reward_infos: &[RewardInfo; NUM_REWARDS],
) -> ([u64; NUM_REWARDS]) {
    // Update reward accrued to the position
    let reward_growths_inside = tick::get_reward_growths_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        current_tick,
        &updated_reward_infos,
    );
    procotol_position_state.update_reward_growths_inside(reward_growths_inside);
    procotol_position_state.reward_growth_inside.clone()
}
