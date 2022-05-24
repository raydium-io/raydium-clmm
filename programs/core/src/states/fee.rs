use anchor_lang::prelude::*;

pub const FEE_SEED: &str = "fee";

/// Stores a fee amount and tick spacing pair enabled by the protocol owner
///
/// A fee amount can never be removed, so this value should be hard coded
/// or cached in the calling context
///
/// PDA of `[FEE_SEED, fee]`
///
#[account]
#[derive(Default, Debug)]
pub struct FeeState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The enabled fee, denominated in hundredths of a bip (10^-6)
    pub fee: u32,

    /// The minimum number of ticks between initialized ticks for pools
    /// created with the given fee
    pub tick_spacing: u16,
}

/// Emitted when a new fee amount is enabled for pool creation via the factory
#[event]
pub struct FeeAmountEnabled {
    /// The enabled fee, denominated in hundredths of a bip (10^-6)
    #[index]
    pub fee: u32,

    /// The minimum number of ticks between initialized ticks for pools
    /// created with the given fee
    #[index]
    pub tick_spacing: u16,
}
