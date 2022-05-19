use anchor_lang::prelude::*;

pub const DEFAULT_AMOUNT_IN_CACHED: u64 = u64::MAX;

#[account]
#[derive(Default)]
pub struct SwapRouterState {
    pub bump: u8,
    pub core: Pubkey,
    /// Cache for exact output swaps
    pub amount_in_cached: u64,
}
