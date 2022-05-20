use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseTickAccount<'info> {
    /// The tick account to be initialized
    #[account(
        mut,
        close = recipient,
        constraint = tick_state.load()?.is_clear()
    )]
    pub tick_state: AccountLoader<'info, TickState>,

    /// Destination for reclaimed lamports
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub recipient: UncheckedAccount<'info>,
}
