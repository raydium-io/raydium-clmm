use crate::error::ErrorCode;
use crate::states::pool::REWARD_NUM;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use crate::util::{parse_remaining_accounts, reward_hook_type, RemainingAccountsInfo};
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
#[instruction(reward_index: u8)]
pub struct CollectReward<'info> {
    /// The position NFT holder
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        constraint = nft_account.amount == 1,
        token::authority = nft_owner,
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The reward vault for `reward_index`. Checked against `pool_state` in the handler
    /// rather than in a constraint, so an out-of-range index errors instead of panicking on
    /// the array index.
    #[account(mut)]
    pub reward_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, token::mint = reward_vault_mint)]
    pub recipient_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = reward_token_vault.mint)]
    pub reward_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    // remaining accounts: one `TransferHookReward<reward_index>` segment
}

/// Pay out one reward slot for a position.
///
/// Split out of `decrease_liquidity`: three reward legs plus both token legs overflow the 64
/// account-lock limit once the mints carry hooks, and one index at a time keeps a reward whose
/// hook refuses transfers from blocking the others or the principal.
///
/// Accrual happens elsewhere - `reward_amount_owed` is settled when liquidity changes.
pub fn collect_reward<'info>(
    ctx: Context<'info, CollectReward<'info>>,
    reward_index: u8,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let index = reward_index as usize;
    require!(index < REWARD_NUM, ErrorCode::InvalidRewardIndex);

    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[reward_hook_type(reward_index)?],
    )?;

    {
        let pool_state = ctx.accounts.pool_state.load()?;
        require!(
            pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward),
            ErrorCode::NotApproved
        );
        require_keys_eq!(
            ctx.accounts.reward_token_vault.key(),
            pool_state.reward_infos[index].token_vault
        );
    }
    require_keys_eq!(
        ctx.accounts.reward_token_vault.mint,
        ctx.accounts.recipient_token_account.mint
    );

    let reward_amount_owed = ctx.accounts.personal_position.reward_infos[index].reward_amount_owed;
    if reward_amount_owed == 0 {
        return Ok(());
    }
    ctx.accounts
        .pool_state
        .load()?
        .check_unclaimed_reward(index, reward_amount_owed)?;

    // A vault can hold less than is owed when emissions ran ahead of funding; pay what is
    // there and leave the rest owed, as the bundled path did.
    let transfer_amount = reward_amount_owed.min(ctx.accounts.reward_token_vault.amount);
    if transfer_amount == 0 {
        return Ok(());
    }

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.reward_token_vault.to_account_info(),
        &ctx.accounts.recipient_token_account.to_account_info(),
        Some(ctx.accounts.reward_vault_mint.clone()),
        &ctx.accounts.token_program,
        Some(ctx.accounts.token_program_2022.to_account_info()),
        parsed.hook_reward[index],
        transfer_amount,
    )?;

    ctx.accounts.personal_position.reward_infos[index].reward_amount_owed = reward_amount_owed
        .checked_sub(transfer_amount)
        .ok_or(ErrorCode::CalculateOverflow)?;
    ctx.accounts
        .pool_state
        .load_mut()?
        .add_reward_claimed(index, transfer_amount)?;

    Ok(())
}
