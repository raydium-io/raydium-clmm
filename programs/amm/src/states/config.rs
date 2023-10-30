use crate::error::ErrorCode;
use anchor_lang::prelude::*;

pub const AMM_CONFIG_SEED: &str = "amm_config";

pub const FEE_RATE_DENOMINATOR_VALUE: u32 = 1_000_000;

/// Holds the current owner of the factory
#[account]
#[derive(Default, Debug)]
pub struct AmmConfig {
    /// Bump to identify PDA
    pub bump: u8,
    pub index: u16,
    /// Address of the protocol owner
    pub owner: Pubkey,
    /// The protocol fee
    pub protocol_fee_rate: u32,
    /// The trade fee, denominated in hundredths of a bip (10^-6)
    pub trade_fee_rate: u32,
    /// The tick spacing
    pub tick_spacing: u16,
    /// The fund fee, denominated in hundredths of a bip (10^-6)
    pub fund_fee_rate: u32,
    // padding space for upgrade
    pub padding_u32: u32,
    pub fund_owner: Pubkey,
    pub padding: [u64; 3],
}

impl AmmConfig {
    pub const LEN: usize = 8 + 1 + 2 + 32 + 4 + 4 + 2 + 64;

    pub fn is_authorized<'info>(
        &self,
        signer: &Signer<'info>,
        expect_pubkey: Pubkey,
    ) -> Result<()> {
        require!(
            signer.key() == self.owner || expect_pubkey == signer.key(),
            ErrorCode::NotApproved
        );
        Ok(())
    }
}

/// Emitted when create or update a config
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ConfigChangeEvent {
    pub index: u16,
    #[index]
    pub owner: Pubkey,
    pub protocol_fee_rate: u32,
    pub trade_fee_rate: u32,
    pub tick_spacing: u16,
    pub fund_fee_rate: u32,
    pub fund_owner: Pubkey,
}
