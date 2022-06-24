use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreateProtocolPosition<'info> {
    /// Pays to create position account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// The address of the position owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub amm_config: UncheckedAccount<'info>,

    /// Create a position account for this pool
    pub pool_state: Box<Account<'info, PoolState>>,

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
            pool_state.token_mint_0.as_ref(),
            pool_state.token_mint_1.as_ref(),
            &pool_state.fee.to_be_bytes(),
            amm_config.key().as_ref(),
            &tick_lower_state.tick.to_be_bytes(),
            &tick_upper_state.tick.to_be_bytes(),
        ],
        bump,
        payer = signer,
        space = ProcotolPositionState::LEN
    )]
    pub position_state: Account<'info, ProcotolPositionState>,

    /// Program to initialize the position account
    pub system_program: Program<'info, System>,
}
