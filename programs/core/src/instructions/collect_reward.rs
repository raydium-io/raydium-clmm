use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct CollectReward<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// The program account of the NFT for which tokens are being collected
    #[account(mut)]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

    /// The program account for the liquidity pool from which fees are collected
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Reward vault of reward index
    #[account(
        mut,
        address = pool_state.reward_infos[reward_index as usize].reward_token_vault
    )]
    pub reward_token_vault: Box<Account<'info, TokenAccount>>,

    /// The destination token account for the collected amount_0
    #[account(
        mut,
        token::mint = reward_token_vault.mint
    )]
    pub recipient_token_account: Account<'info, TokenAccount>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn collect_reward<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CollectReward<'info>>,
    reward_index: u8,
    amount_desired: u64,
) -> Result<()> {
    let index = reward_index as usize;
    assert!(index < NUM_REWARDS);

    let tokenized_position = &mut ctx.accounts.personal_position_state;
    let reward_amount_owed = tokenized_position.reward_infos[index].reward_amount_owed;
    let max_transfer_amount = if reward_amount_owed > ctx.accounts.reward_token_vault.amount {
        ctx.accounts.reward_token_vault.amount
    } else {
        reward_amount_owed
    };

    require!(
        amount_desired <= max_transfer_amount,
        ErrorCode::InvalidRewardDesiredAmount
    );

    let transfer_amount = if amount_desired == 0 {
        max_transfer_amount
    } else {
        amount_desired
    };

    tokenized_position.reward_infos[index].reward_amount_owed =
        reward_amount_owed.checked_sub(transfer_amount).unwrap();

    transfer_from_pool_vault_to_user(
        &mut ctx.accounts.pool_state,
        &ctx.accounts.reward_token_vault,
        &ctx.accounts.recipient_token_account,
        &ctx.accounts.token_program,
        transfer_amount,
    )?;

    msg!(
        "collect reward amount: {}, index: {}",
        transfer_amount,
        reward_index
    );

    emit!(CollectRewardEvent {
        reward_mint: ctx.accounts.reward_token_vault.mint,
        reward_amount: transfer_amount,
        reward_index
    });

    Ok(())
}
