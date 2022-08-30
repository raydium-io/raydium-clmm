// use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct ResetSqrtPrice<'info> {
    /// Only admin has the authority to reset initial price
    #[account(address = crate::admin::id())]
    pub owner: Signer<'info>,

    /// Initialize an account to store the pool state
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

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

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// The destination token account for receive amount_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub recipient_token_account_0: Box<Account<'info, TokenAccount>>,

    /// The destination token account for receive amount_1
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub recipient_token_account_1: Box<Account<'info, TokenAccount>>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn reset_sqrt_price(ctx: Context<ResetSqrtPrice>, sqrt_price_x64: u128) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let mut observation_state = ctx.accounts.observation_state.load_mut()?;

    // reset observation
    observation_state.initialized = false;
    observation_state.observations = [Observation::default(); OBSERVATION_NUM];
    // update pool
    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64)?;
    pool_state.pool_check_reset(sqrt_price_x64, tick)?;

    transfer_from_pool_vault_to_user(
        &pool_state,
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_0,
        &ctx.accounts.recipient_token_account_0,
        &ctx.accounts.token_program,
        ctx.accounts.token_vault_0.amount,
    )?;
    transfer_from_pool_vault_to_user(
        &pool_state,
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_1,
        &ctx.accounts.recipient_token_account_1,
        &ctx.accounts.token_program,
        ctx.accounts.token_vault_1.amount,
    )?;

    Ok(())
}
