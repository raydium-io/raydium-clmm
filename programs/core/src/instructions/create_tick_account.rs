use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(tick: i32)]
pub struct CreateTickAccount<'info> {
    /// Pays to create tick account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// Create a tick account for this pool
    pub pool_state: Account<'info, PoolState>,

    /// The tick account to be initialized
    #[account(
        init,
        seeds = [
            TICK_SEED.as_bytes(),
            pool_state.token_mint_0.as_ref(),
            pool_state.token_mint_1.as_ref(),
            &pool_state.fee.to_be_bytes(),
            &tick.to_be_bytes()
        ],
        bump,
        payer = signer,
        space = TickState::LEN
    )]
    pub tick_state: Account<'info, TickState>,

    /// Program to initialize the tick account
    pub system_program: Program<'info, System>,
}
