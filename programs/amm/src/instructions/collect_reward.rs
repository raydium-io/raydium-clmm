use crate::error::ErrorCode;
use crate::states::*;
use crate::util::{get_recent_epoch, transfer_from_pool_vault_to_user};
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

    /// Holds the position's lower tick, to settle what accrued since the position last changed.
    #[account(constraint = tick_array_lower.load()?.pool_id == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Holds the position's upper tick.
    #[account(constraint = tick_array_upper.load()?.pool_id == pool_state.key())]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    /// The reward vault for `reward_index`; matched against `pool_state` in the handler.
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
/// Settles the slot up to now first, so the payout includes what accrued since the position
/// last changed.
pub fn collect_reward<'info>(
    ctx: Context<'info, CollectReward<'info>>,
    reward_index: u8,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[reward_hook_type(reward_index)?],
    )?;
    let index = reward_index as usize;

    let transfer_amount = {
        let mut pool_state = ctx.accounts.pool_state.load_mut()?;
        require!(
            pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward),
            ErrorCode::NotApproved
        );
        require_keys_eq!(
            ctx.accounts.reward_token_vault.key(),
            pool_state.reward_infos[index].token_vault
        );

        let reward_infos = pool_state.update_reward_infos(Clock::get()?.unix_timestamp as u64)?;
        let position = &mut ctx.accounts.personal_position;
        let tick_lower = *ctx
            .accounts
            .tick_array_lower
            .load()?
            .get_tick_state(position.tick_lower_index, pool_state.tick_spacing)?;
        let tick_upper = *ctx
            .accounts
            .tick_array_upper
            .load()?
            .get_tick_state(position.tick_upper_index, pool_state.tick_spacing)?;
        position.update_rewards(
            get_reward_growths_inside(
                &tick_lower,
                &tick_upper,
                pool_state.tick_current,
                &reward_infos,
            ),
            true,
            get_recent_epoch()?,
        )?;

        let reward_amount_owed = position.reward_infos[index].reward_amount_owed;
        pool_state.check_unclaimed_reward(index, reward_amount_owed)?;

        let amount = reward_amount_owed.min(ctx.accounts.reward_token_vault.amount);
        position.reward_infos[index].reward_amount_owed = reward_amount_owed - amount;
        pool_state.add_reward_claimed(index, amount)?;
        amount
    };
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
    )
}
