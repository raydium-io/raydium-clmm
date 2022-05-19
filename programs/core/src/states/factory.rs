use anchor_lang::prelude::*;

/// Holds the current owner of the factory
///
/// # The owner can
///
/// 1. Set and collect a pool's protocol fees
/// 2. Enable a new fee amount for pool creation
/// 3. Set another address as an owner
///
/// PDA of `[]`
///
#[account(zero_copy)]
#[derive(Default)]
#[repr(packed)]
pub struct FactoryState {
    /// Bump to identify PDA
    pub bump: u8,

    /// Address of the protocol owner
    pub owner: Pubkey,

    /// The global protocol fee
    pub fee_protocol: u8,
}

/// Emitted when the owner of the factory is changed
#[event]
pub struct OwnerChanged {
    /// The owner before the owner was changed
    #[index]
    pub old_owner: Pubkey,

    /// The owner after the owner was changed
    #[index]
    pub new_owner: Pubkey,
}

/// Emitted when the protocol fee is changed for a pool
#[event]
pub struct SetFeeProtocolEvent {
    /// The previous value of the protocol fee
    pub fee_protocol_old: u8,

    /// The updated value of the protocol fee
    pub fee_protocol: u8,
}
