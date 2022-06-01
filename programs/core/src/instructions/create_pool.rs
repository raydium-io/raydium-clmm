use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use std::{mem::size_of, ops::DerefMut};

#[derive(Accounts)]
pub struct CreatePool<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub pool_creator: Signer<'info>,
    /// Serum market id
    /// CHECK:
    pub serum_market: UncheckedAccount<'info>,
    /// Initialize an account to store the pool state
    #[account(
        init,
        seeds = [
            POOL_SEED.as_bytes(),
            serum_market.key().as_ref(),
            token_mint_0.key().as_ref(),
            token_mint_1.key().as_ref(),
            &fee_state.fee.to_be_bytes()
        ],
        bump,
        payer = pool_creator,
        space = 8 + size_of::<PoolState>()
    )]
    pub pool_state: Account<'info, PoolState>,
    /// Desired token pair for the pool
    /// token_0 mint address should be smaller than token_1 address
    #[account(
        constraint = token_mint_0.key() < token_mint_1.key()
    )]
    pub token_mint_0: Box<Account<'info, Mint>>,
    pub token_mint_1: Box<Account<'info, Mint>>,
    /// Token_0 vault
    #[account(
        init,
        seeds =[
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_mint_0.key().as_ref(),
            &fee_state.fee.to_be_bytes(),
        ],
        bump,
        payer = pool_creator,
        token::mint = token_mint_0,
        token::authority = pool_state
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,
    /// Token_1 vault
    #[account(
        init,
        seeds =[
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_mint_1.key().as_ref(),
            &fee_state.fee.to_be_bytes(),
        ],
        bump,
        payer = pool_creator,
        token::mint = token_mint_1,
        token::authority = pool_state
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,
    /// Stores the desired fee for the pool
    pub fee_state: Account<'info, FeeState>,

    /// Initialize an account to store oracle observations
    #[account(
        init,
        seeds = [
            &OBSERVATION_SEED.as_bytes(),
            token_mint_0.key().as_ref(),
            token_mint_1.key().as_ref(),
            &fee_state.fee.to_be_bytes(),
            &0_u16.to_be_bytes(),
        ],
        bump,
        payer = pool_creator,
        space = 8 + size_of::<ObservationState>()
    )]
    pub initial_observation_state: Account<'info, ObservationState>,
    /// Spl token program
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_pool(ctx: Context<CreatePool>, sqrt_price_x32: u64) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.deref_mut();
    let tick = tick_math::get_tick_at_sqrt_ratio(sqrt_price_x32)?;

    pool_state.bump = *ctx.bumps.get("pool_state").unwrap();
    pool_state.market = ctx.accounts.serum_market.key();
    pool_state.token_mint_0 = ctx.accounts.token_mint_0.key();
    pool_state.token_mint_1 = ctx.accounts.token_mint_1.key();
    pool_state.token_vault_0 = ctx.accounts.token_vault_0.key();
    pool_state.token_vault_1 = ctx.accounts.token_vault_1.key();
    pool_state.fee = ctx.accounts.fee_state.fee;
    pool_state.tick_spacing = ctx.accounts.fee_state.tick_spacing;
    pool_state.sqrt_price_x32 = sqrt_price_x32;
    pool_state.tick = tick;
    pool_state.observation_cardinality = 1;
    pool_state.observation_cardinality_next = 1;

    let initial_observation_state = ctx.accounts.initial_observation_state.deref_mut();
    initial_observation_state.bump = *ctx.bumps.get("initial_observation_state").unwrap();
    initial_observation_state.block_timestamp = oracle::_block_timestamp();
    initial_observation_state.initialized = true;

    emit!(PoolCreatedAndInitialized {
        token_0: ctx.accounts.token_mint_0.key(),
        token_1: ctx.accounts.token_mint_1.key(),
        fee: ctx.accounts.fee_state.fee,
        tick_spacing: ctx.accounts.fee_state.tick_spacing,
        pool_state: ctx.accounts.pool_state.key(),
        sqrt_price_x32,
        tick,
        market: ctx.accounts.serum_market.key(),
        vault_0: ctx.accounts.token_vault_0.key(),
        vault_1: ctx.accounts.token_vault_1.key(),
    });
    Ok(())
}
