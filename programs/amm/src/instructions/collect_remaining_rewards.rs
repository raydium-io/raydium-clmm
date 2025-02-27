use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Token},
    token_interface::{Mint, Token2022, TokenAccount},
};

/// Memo msg for collect remaining
pub const COLLECT_REMAINING_MEMO_MSG: &'static [u8] = b"raydium_collect_remaining";

#[derive(Accounts)]
pub struct CollectRemainingRewards<'info> {
    /// The founder who init reward info previously
    pub reward_funder: Signer<'info>,
    /// The funder's reward token account
    #[account(mut)]
    pub funder_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    /// Set reward for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    /// Reward vault transfer remaining token to founder token account
    pub reward_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// The mint of reward token vault
    #[account(
        address = reward_token_vault.mint
    )]
    pub reward_vault_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,

    /// memo program
    /// CHECK:
    #[account(
        address = spl_memo::id()
    )]
    pub memo_program: UncheckedAccount<'info>,
}

pub fn collect_remaining_rewards(
    ctx: Context<CollectRemainingRewards>,
    reward_index: u8,
) -> Result<()> {
    // invoke_memo_instruction(
    //     COLLECT_REMAINING_MEMO_MSG,
    //     ctx.accounts.memo_program.to_account_info(),
    // )?;
    let amount_remaining = get_remaining_reward_amount(
        &ctx.accounts.pool_state,
        &ctx.accounts.reward_token_vault,
        &ctx.accounts.reward_funder.key(),
        reward_index,
    )?;

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.reward_token_vault.to_account_info(),
        &ctx.accounts.funder_token_account.to_account_info(),
        Some(ctx.accounts.reward_vault_mint.clone()),
        &ctx.accounts.token_program,
        Some(ctx.accounts.token_program_2022.to_account_info()),
        amount_remaining,
    )?;

    Ok(())
}

fn get_remaining_reward_amount(
    pool_state_loader: &AccountLoader<PoolState>,
    reward_token_vault: &InterfaceAccount<TokenAccount>,
    reward_funder: &Pubkey,
    reward_index: u8,
) -> Result<u64> {
    let current_timestamp = u64::try_from(Clock::get()?.unix_timestamp).unwrap();
    let mut pool_state = pool_state_loader.load_mut()?;
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
    require_keys_eq!(reward_funder.key(), pool_state.owner);
    require_keys_eq!(reward_token_vault.key(), reward_info.token_vault);

    let amount_remaining = reward_token_vault
        .amount
        .checked_sub(
            reward_info
                .reward_total_emissioned
                .checked_sub(reward_info.reward_claimed)
                .unwrap(),
        )
        .unwrap();

    Ok(amount_remaining)
}
