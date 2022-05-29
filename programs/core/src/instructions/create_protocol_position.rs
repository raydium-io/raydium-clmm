use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use std::mem::size_of;

#[derive(Accounts)]
pub struct CreateProtocolPosition<'info> {
    /// Pays to create position account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The address of the position owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub factory_state: UncheckedAccount<'info>,

    /// Create a position account for this pool
    pub pool_state: Account<'info, PoolState>,

    /// The lower tick boundary of the position
    pub tick_lower_state: Account<'info, TickState>,

    /// The upper tick boundary of the position
    #[account(
        constraint = tick_lower_state.tick < tick_upper_state.tick @ErrorCode::InvaildTickIndex
    )]
    pub tick_upper_state: Account<'info, TickState>,

    /// The position account to be initialized
    #[account(
        init,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.token_0.as_ref(),
            pool_state.token_1.as_ref(),
            &pool_state.fee.to_be_bytes(),
            factory_state.key().as_ref(),
            &tick_lower_state.tick.to_be_bytes(),
            &tick_upper_state.tick.to_be_bytes(),
        ],
        bump,
        payer = signer,
        space = 8 + size_of::<ProcotolPositionState>()
    )]
    pub position_state: Account<'info, ProcotolPositionState>,

    /// Program to initialize the position account
    pub system_program: Program<'info, System>,
}
