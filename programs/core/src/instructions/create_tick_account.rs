use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(tick: i32)]
pub struct CreateTickAccount<'info> {
    /// Pays to create tick account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// Create a tick account for this pool
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The tick account to be initialized
    #[account(
        init,
        seeds = [
            TICK_SEED.as_bytes(),
            pool_state.key().as_ref(),
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
