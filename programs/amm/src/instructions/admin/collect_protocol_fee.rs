use crate::decrease_liquidity::check_unclaimed_fees_and_vault;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
#[derive(Accounts)]
pub struct CollectProtocolFee<'info> {
    /// Only admin can collect fee now
    #[account(address = crate::admin::id())]
    pub owner: Signer<'info>,

    /// Pool state stores accumulated protocol fee amount
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Account<'info, TokenAccount>,

    /// The address that receives the collected token_0 protocol fees
    #[account(mut)]
    pub recipient_token_account_0: Account<'info, TokenAccount>,

    /// The address that receives the collected token_1 protocol fees
    #[account(mut)]
    pub recipient_token_account_1: Account<'info, TokenAccount>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

pub fn collect_protocol_fee(
    ctx: Context<CollectProtocolFee>,
    amount_0_requested: u64,
    amount_1_requested: u64,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;

    let amount_0 = amount_0_requested.min(pool_state.protocol_fees_token_0);
    let amount_1 = amount_1_requested.min(pool_state.protocol_fees_token_1);

    pool_state.protocol_fees_token_0 = pool_state
        .protocol_fees_token_0
        .checked_sub(amount_0)
        .unwrap();
    pool_state.protocol_fees_token_1 = pool_state
        .protocol_fees_token_1
        .checked_sub(amount_1)
        .unwrap();

    pool_state.total_fees_claimed_token_0 = pool_state
        .total_fees_claimed_token_0
        .checked_add(amount_0)
        .unwrap();
    pool_state.total_fees_claimed_token_1 = pool_state
        .total_fees_claimed_token_1
        .checked_add(amount_1)
        .unwrap();

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_0,
        &ctx.accounts.recipient_token_account_0,
        &ctx.accounts.token_program,
        amount_0,
    )?;

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_1,
        &ctx.accounts.recipient_token_account_1,
        &ctx.accounts.token_program,
        amount_1,
    )?;

    check_unclaimed_fees_and_vault(
        &ctx.accounts.pool_state,
        &mut ctx.accounts.token_vault_0,
        &mut ctx.accounts.token_vault_1,
    )?;

    emit!(CollectProtocolFeeEvent {
        pool_state: ctx.accounts.pool_state.key(),
        recipient_token_account_0: ctx.accounts.recipient_token_account_0.key(),
        recipient_token_account_1: ctx.accounts.recipient_token_account_1.key(),
        amount_0,
        amount_1,
    });

    Ok(())
}
