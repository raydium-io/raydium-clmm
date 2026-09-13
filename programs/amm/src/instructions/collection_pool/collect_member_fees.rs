use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

/// Collects protocol (kind 0) or fund (kind 1) fees accrued in a non-base member vault.
#[derive(Accounts)]
pub struct CollectMemberFees<'info> {
    #[account(
        constraint = (owner.key() == amm_config.owner || owner.key() == amm_config.fund_owner || owner.key() == crate::admin::ID) @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump = pool_members.bump,
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    #[account(mut)]
    pub member_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = member_vault.mint)]
    pub recipient_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = member_vault.mint)]
    pub vault_mint: Box<InterfaceAccount<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
}

pub fn collect_member_fees(ctx: Context<CollectMemberFees>, member_index: u8, kind: u8) -> Result<()> {
    let idx = member_index as usize;
    let pm = &mut ctx.accounts.pool_members;
    require!(idx > 0 && idx < pm.n as usize, ErrorCode::InvalidPoolMember);
    require_keys_eq!(pm.members[idx].vault, ctx.accounts.member_vault.key(), ErrorCode::InvalidPoolMember);
    let amount = match kind {
        0 => std::mem::take(&mut pm.members[idx].protocol_fees_owed),
        1 => std::mem::take(&mut pm.members[idx].fund_fees_owed),
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    };
    require_gt!(amount, 0, ErrorCode::NoFeeToCollect);
    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.member_vault.to_account_info(),
        &ctx.accounts.recipient_token_account.to_account_info(),
        Some(ctx.accounts.vault_mint.clone()),
        &ctx.accounts.token_program.to_account_info(),
        Some(ctx.accounts.token_program_2022.to_account_info()),
        amount,
    )
}
