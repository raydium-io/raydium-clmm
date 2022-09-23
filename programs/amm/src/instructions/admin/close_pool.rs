use crate::states::*;
use crate::util::{close_account, close_spl_account};
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct ClosePool<'info> {
    /// Only admin has the authority to reset initial price
    #[account(mut, address = crate::admin::id())]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The program account for the oracle observation
    #[account(mut, constraint = observation_state.load()?.pool_id == pool_state.key())]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// Token_0 vault
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// Token_1 vault
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,
    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
}

pub fn close_pool(ctx: Context<ClosePool>) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.load()?;
    let pool_state_seeds = [
        &POOL_SEED.as_bytes(),
        &pool_state.amm_config.as_ref(),
        &pool_state.token_mint_0.to_bytes() as &[u8],
        &pool_state.token_mint_1.to_bytes() as &[u8],
        &[pool_state.bump],
    ];
    close_spl_account(
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_program,
        &[&pool_state_seeds[..]],
    )?;
    close_spl_account(
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.token_program,
        &[&pool_state_seeds[..]],
    )?;

    // close pool account
    close_account(
        &ctx.accounts.observation_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;
    close_account(
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
