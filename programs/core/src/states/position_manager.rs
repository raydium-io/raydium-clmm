use anchor_lang::prelude::*;

#[account(zero_copy)]
#[derive(Default)]
#[repr(packed)]
pub struct PositionManagerState {
    /// Bump to identify PDA
    pub bump: u8,
}
