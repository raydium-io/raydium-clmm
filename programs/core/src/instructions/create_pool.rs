use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;
use std::mem::size_of;

#[derive(Accounts)]
pub struct CreateAndInitPool<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub pool_creator: Signer<'info>,

    /// Desired token pair for the pool
    /// token_0 mint address should be smaller than token_1 address
    #[account(
        constraint = token_0.key() < token_1.key()
    )]
    pub token_0: Box<Account<'info, Mint>>,
    pub token_1: Box<Account<'info, Mint>>,
    /// Stores the desired fee for the pool
    pub fee_state: AccountLoader<'info, FeeState>,

    /// Initialize an account to store the pool state
    #[account(
        init,
        seeds = [
            POOL_SEED.as_bytes(),
            token_0.key().as_ref(),
            token_1.key().as_ref(),
            &fee_state.load()?.fee.to_be_bytes()
        ],
        bump,
        payer = pool_creator,
        space = 8 + size_of::<PoolState>()
    )]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Initialize an account to store oracle observations
    #[account(
        init,
        seeds = [
            &OBSERVATION_SEED.as_bytes(),
            token_0.key().as_ref(),
            token_1.key().as_ref(),
            &fee_state.load()?.fee.to_be_bytes(),
            &0_u16.to_be_bytes(),
        ],
        bump,
        payer = pool_creator,
        space = 8 + size_of::<ObservationState>()
    )]
    pub initial_observation_state: AccountLoader<'info, ObservationState>,

    /// To create a new program account
    pub system_program: Program<'info, System>,

    /// Sysvar for program account and ATA creation
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_and_init_pool(ctx: Context<CreateAndInitPool>, sqrt_price_x32: u64) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_init()?;
    let fee_state = ctx.accounts.fee_state.load()?;
    let tick = tick_math::get_tick_at_sqrt_ratio(sqrt_price_x32)?;

    pool_state.bump = *ctx.bumps.get("pool_state").unwrap();
    pool_state.token_0 = ctx.accounts.token_0.key();
    pool_state.token_1 = ctx.accounts.token_1.key();
    pool_state.fee = fee_state.fee;
    pool_state.tick_spacing = fee_state.tick_spacing;
    pool_state.sqrt_price_x32 = sqrt_price_x32;
    pool_state.tick = tick;
    pool_state.unlocked = true;
    pool_state.observation_cardinality = 1;
    pool_state.observation_cardinality_next = 1;

    let mut initial_observation_state = ctx.accounts.initial_observation_state.load_init()?;
    initial_observation_state.bump = *ctx.bumps.get("initial_observation_state").unwrap();
    initial_observation_state.block_timestamp = oracle::_block_timestamp();
    initial_observation_state.initialized = true;

    // default value 0 for remaining variables

    emit!(PoolCreatedAndInitialized {
        token_0: ctx.accounts.token_0.key(),
        token_1: ctx.accounts.token_1.key(),
        fee: fee_state.fee,
        tick_spacing: fee_state.tick_spacing,
        pool_state: ctx.accounts.pool_state.key(),
        sqrt_price_x32,
        tick,
    });
    Ok(())
}
