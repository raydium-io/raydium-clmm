use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct CollectContext<'info> {
    /// The position owner
    pub owner: Signer<'info>,

    /// The program account for the liquidity pool from which fees are collected
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The lower tick of the position for which to collect fees
    /// CHECK: Safety check performed inside function body
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The upper tick of the position for which to collect fees
    /// CHECK: Safety check performed inside function body
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The position program account to collect fees from
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub position_state: Box<Account<'info, PositionState>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = vault_0.key() == pool_state.token_vault_0
    )]
    pub vault_0: Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = vault_1.key() == pool_state.token_vault_1
    )]
    pub vault_1: Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_0
    #[account(
        mut,
        token::mint = vault_0.mint
    )]
    pub recipient_wallet_0: Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_1
    #[account(
        mut,
        token::mint = vault_1.mint
    )]
    pub recipient_wallet_1: Account<'info, TokenAccount>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn collect(
    ctx: Context<CollectContext>,
    amount_0_requested: u64,
    amount_1_requested: u64,
) -> Result<()> {
    let pool_state_info = ctx.accounts.pool_state.to_account_info();
    let pool_state = &mut ctx.accounts.pool_state;

    pool_state.validate_tick_address(
        &ctx.accounts.tick_lower_state.key(),
        ctx.accounts.tick_lower_state.bump,
        ctx.accounts.tick_lower_state.tick,
    )?;

    pool_state.validate_tick_address(
        &ctx.accounts.tick_upper_state.key(),
        ctx.accounts.tick_upper_state.bump,
        ctx.accounts.tick_upper_state.tick,
    )?;

    pool_state.validate_position_address(
        &ctx.accounts.position_state.key(),
        ctx.accounts.position_state.bump,
        &ctx.accounts.owner.key(),
        ctx.accounts.tick_lower_state.tick,
        ctx.accounts.tick_upper_state.tick,
    )?;

    require!(pool_state.unlocked, ErrorCode::PoolStateLocked);
    pool_state.unlocked = false;

    let position = &mut ctx.accounts.position_state;

    let amount_0 = amount_0_requested.min(position.tokens_owed_0);
    let amount_1 = amount_1_requested.min(position.tokens_owed_1);

    if amount_0 > 0 {
        position.tokens_owed_0 -= amount_0;
        transfer_from_pool_vault_to_user(
            pool_state,
            &ctx.accounts.vault_0,
            &ctx.accounts.recipient_wallet_0,
            &ctx.accounts.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        position.tokens_owed_1 -= amount_1;
        transfer_from_pool_vault_to_user(
            pool_state,
            &ctx.accounts.vault_1,
            &ctx.accounts.recipient_wallet_1,
            &ctx.accounts.token_program,
            amount_1,
        )?;
    }

    emit!(CollectEvent {
        pool_state: pool_state_info.key(),
        owner: ctx.accounts.owner.key(),
        tick_lower: ctx.accounts.tick_lower_state.tick,
        tick_upper: ctx.accounts.tick_upper_state.tick,
        amount_0,
        amount_1,
    });

    pool_state.unlocked = true;
    Ok(())
}
