use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};

#[derive(Accounts)]
pub struct CollectRemainingRewards<'info> {
    /// The founder who init reward info in berfore
    #[account(mut)]
    pub reward_funder: Signer<'info>,
    /// The funder's reward token account
    #[account(mut)]
    pub funder_token_account: Box<Account<'info, TokenAccount>>,
    /// Set reward for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    /// Reward vault transfer remaining token to founder token account
    pub reward_token_vault: Box<Account<'info, TokenAccount>>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
}

pub fn collect_remaining_rewards(
    ctx: Context<CollectRemainingRewards>,
    reward_index: u8,
) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp as u64;
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.update_reward_infos(current_timestamp)?;

    let reward_info = pool_state.reward_infos[reward_index as usize];
    if !reward_info.initialized() {
        return err!(ErrorCode::UnInitializedRewardInfo);
    }
    require_eq!(
        reward_info.last_update_time,
        reward_info.end_time,
        ErrorCode::NotApproved
    );
    require_keys_eq!(ctx.accounts.reward_funder.key(), reward_info.authority);
    require_keys_eq!(
        ctx.accounts.reward_token_vault.key(),
        reward_info.token_vault
    );

    let amount_remaining = ctx
        .accounts
        .reward_token_vault
        .amount
        .checked_sub(reward_info.reward_total_emissioned)
        .unwrap();

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.reward_token_vault,
        &ctx.accounts.funder_token_account,
        &ctx.accounts.token_program,
        amount_remaining,
    )?;

    Ok(())
}
