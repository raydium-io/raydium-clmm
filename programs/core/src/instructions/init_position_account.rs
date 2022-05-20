use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use std::mem::size_of;

#[derive(Accounts)]
pub struct InitPositionAccount<'info> {
    /// Pays to create position account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The address of the position owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub recipient: UncheckedAccount<'info>,

    /// Create a position account for this pool
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The lower tick boundary of the position
    pub tick_lower_state: AccountLoader<'info, TickState>,

    /// The upper tick boundary of the position
    #[account(
        constraint = tick_lower_state.load()?.tick < tick_upper_state.load()?.tick @ErrorCode::TLU
    )]
    pub tick_upper_state: AccountLoader<'info, TickState>,

    /// The position account to be initialized
    #[account(
        init,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.load()?.token_0.as_ref(),
            pool_state.load()?.token_1.as_ref(),
            &pool_state.load()?.fee.to_be_bytes(),
            recipient.key().as_ref(),
            &tick_lower_state.load()?.tick.to_be_bytes(),
            &tick_upper_state.load()?.tick.to_be_bytes(),
        ],
        bump,
        payer = signer,
        space = 8 + size_of::<PositionState>()
    )]
    pub position_state: AccountLoader<'info, PositionState>,

    /// Program to initialize the position account
    pub system_program: Program<'info, System>,
}
