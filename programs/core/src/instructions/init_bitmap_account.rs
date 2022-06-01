use crate::states::*;
use anchor_lang::prelude::*;
use std::mem::size_of;

#[derive(Accounts)]
#[instruction(word_pos: i16)]
pub struct InitBitmapAccount<'info> {
    /// Pays to create bitmap account
    #[account(mut)]
    pub signer: Signer<'info>,

    /// Create a new bitmap account for this pool
    pub pool_state: Account<'info, PoolState>,

    /// The bitmap account to be initialized
    #[account(
        init,
        seeds = [
            BITMAP_SEED.as_bytes(),
            pool_state.token_mint_0.as_ref(),
            pool_state.token_mint_1.as_ref(),
            &pool_state.fee.to_be_bytes(),
            &word_pos.to_be_bytes()
        ],
        bump,
        payer = signer,
        space = 8 + size_of::<TickBitmapState>()
    )]
    pub bitmap_state: AccountLoader<'info, TickBitmapState>,

    /// Program to initialize the tick account
    pub system_program: Program<'info, System>,
}
