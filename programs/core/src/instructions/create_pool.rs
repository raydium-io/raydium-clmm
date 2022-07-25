use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use std::{mem::size_of, ops::DerefMut};

#[derive(Accounts)]
pub struct CreatePool<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub pool_creator: Signer<'info>,
    /// Which config the pool belongs to.
    pub amm_config: Box<Account<'info, AmmConfig>>,
    /// Initialize an account to store the pool state
    #[account(
        init,
        seeds = [
            POOL_SEED.as_bytes(),
            amm_config.key().as_ref(),
            token_mint_0.key().as_ref(),
            token_mint_1.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        space = PoolState::LEN
    )]
    pub pool_state: Box<Account<'info, PoolState>>,
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
        ],
        bump,
        payer = pool_creator,
        token::mint = token_mint_1,
        token::authority = pool_state
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// Initialize an account to store oracle observations
    #[account(
        init,
        seeds = [
            &OBSERVATION_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        space = 8 + size_of::<ObservationState>()
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,
    /// Spl token program
    pub token_program: Program<'info, Token>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_pool(ctx: Context<CreatePool>, sqrt_price_x64: u128) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.deref_mut();

    let tick = tick_math::get_tick_at_sqrt_ratio(sqrt_price_x64)?;
    #[cfg(feature = "enable-log")]
    msg!(
        "create pool, init_price: {}, init_tick:{}",
        sqrt_price_x64,
        tick
    );
    pool_state.bump = *ctx.bumps.get("pool_state").unwrap();
    pool_state.amm_config = ctx.accounts.amm_config.key();
    pool_state.owner = ctx.accounts.pool_creator.key();
    pool_state.token_mint_0 = ctx.accounts.token_mint_0.key();
    pool_state.token_mint_1 = ctx.accounts.token_mint_1.key();
    pool_state.mint_0_decimals = ctx.accounts.token_mint_0.decimals;
    pool_state.mint_1_decimals = ctx.accounts.token_mint_1.decimals;
    pool_state.token_vault_0 = ctx.accounts.token_vault_0.key();
    pool_state.token_vault_1 = ctx.accounts.token_vault_1.key();
    pool_state.tick_spacing = ctx.accounts.amm_config.tick_spacing;
    pool_state.sqrt_price_x64 = sqrt_price_x64;
    pool_state.tick_current = tick;
    pool_state.observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
    pool_state.reward_infos = [RewardInfo::new(ctx.accounts.pool_creator.key()); REWARD_NUM];
    pool_state.observation_key = ctx.accounts.observation_state.key();

    emit!(PoolCreatedEvent {
        token_mint_0: ctx.accounts.token_mint_0.key(),
        token_mint_1: ctx.accounts.token_mint_1.key(),
        tick_spacing: ctx.accounts.amm_config.tick_spacing,
        pool_state: ctx.accounts.pool_state.key(),
        sqrt_price_x64,
        tick,
        token_vault_0: ctx.accounts.token_vault_0.key(),
        token_vault_1: ctx.accounts.token_vault_1.key(),
    });
    Ok(())
}
