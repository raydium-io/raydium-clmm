use crate::{ObservationState, PoolState};
use anchor_lang::prelude::*;
use std::str::FromStr;

use crate::states::*;

#[derive(Accounts)]
pub struct CreateObservation<'info> {
    /// The position owner or delegated authority
    #[account(mut,
        address = Pubkey::from_str("RayyNewmYYuLtrZvS1HKxYc5EXAcs6EqDpgoqrCETPj").unwrap()
    )]
    pub payer: Signer<'info>,

    /// pool state
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Initialize an account to store oracle observations, the account must be created off-chain, constract will initialzied it
    #[account(
        init,
        seeds = [
            OBSERVATION_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = payer,
        space = ObservationState::LEN
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn create_observation(ctx: Context<CreateObservation>) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.observation_key = ctx.accounts.observation_state.key();
    pool_state.padding3 = 0;
    pool_state.padding4 = 0;

    let mut observation_state = ctx.accounts.observation_state.load_init()?;
    observation_state.initialize(ctx.accounts.pool_state.key())?;
    Ok(())
}
