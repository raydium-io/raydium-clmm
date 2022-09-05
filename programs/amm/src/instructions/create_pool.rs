use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use std::mem::size_of;

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
        space = 8 + size_of::<PoolState>()
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Token_0 mint, the key must grater then token_1 mint.
    #[account(
        constraint = token_mint_0.key() < token_mint_1.key()
    )]
    pub token_mint_0: Box<Account<'info, Mint>>,

    /// Token_1 mint
    pub token_mint_1: Box<Account<'info, Mint>>,

    /// Token_0 vault for the pool
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

    /// Token_1 vault for the pool
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

    /// CHECK: Initialize an account to store oracle observations, the account must be created off-chain, constract will initialzied it
    pub observation_state: UncheckedAccount<'info>,

    /// Spl token program
    pub token_program: Program<'info, Token>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_pool(ctx: Context<CreatePool>, sqrt_price_x64: u128) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_init()?;
    let observation_state_loader = initialize_observation_account(
        ctx.accounts.observation_state.to_account_info(),
        &crate::id(),
    )?;
    let mut observation_state = observation_state_loader.load_mut()?;

    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64)?;
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
    pool_state.mint_decimals_0 = ctx.accounts.token_mint_0.decimals;
    pool_state.mint_decimals_1 = ctx.accounts.token_mint_1.decimals;
    pool_state.token_vault_0 = ctx.accounts.token_vault_0.key();
    pool_state.token_vault_1 = ctx.accounts.token_vault_1.key();
    pool_state.tick_spacing = ctx.accounts.amm_config.tick_spacing;
    pool_state.sqrt_price_x64 = sqrt_price_x64;
    pool_state.tick_current = tick;
    pool_state.observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
    pool_state.reward_infos = [RewardInfo::new(ctx.accounts.pool_creator.key()); REWARD_NUM];

    require_eq!(observation_state.initialized, false);
    require_keys_eq!(observation_state.amm_pool, Pubkey::default());
    pool_state.observation_key = ctx.accounts.observation_state.key();
    observation_state.amm_pool = ctx.accounts.pool_state.key();

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

fn initialize_observation_account<'info>(
    observation_account_info: AccountInfo<'info>,
    program_id: &Pubkey,
) -> Result<AccountLoader<'info, ObservationState>> {
    let observation_loader = AccountLoader::<ObservationState>::try_from_unchecked(
        program_id,
        &observation_account_info,
    )?;
    observation_loader.exit(&crate::id())?;
    Ok(observation_loader)
}
