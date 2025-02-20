use crate::error::ErrorCode;
use anchor_lang::prelude::*;

pub const AMM_CONFIG_SEED: &str = "amm_config";

pub const FEE_RATE_DENOMINATOR_VALUE: u32 = 1_000_000;
// 6 hours
pub const TIME_DECAY_SNIPER_FEE: i64 = 6 * 60 * 60;

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
    // track the time when liquidity was added
    pub liquidity_added_time: i64,

    pub padding: [u64; 2],
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

// Starts with 100% fee initially, decays linealy
pub fn calculate_dynamic_fee(amm_config: &AmmConfig) -> Result<(u32, u32, u32)> {
    let current_time = Clock::get().unwrap().unix_timestamp;
    let elapsed_time = current_time - amm_config.liquidity_added_time;
    require_gte!(elapsed_time, 0);

    let (dynamic_trade_fee_rate, dynamic_protocol_fee_rate, dynamic_fund_fee_rate) =
        if elapsed_time >= TIME_DECAY_SNIPER_FEE {
            (
                amm_config.trade_fee_rate,
                amm_config.protocol_fee_rate,
                amm_config.fund_fee_rate,
            )
        } else {
            let fee_delta = FEE_RATE_DENOMINATOR_VALUE - amm_config.trade_fee_rate;
            let fee_reduction = (fee_delta as i64 * elapsed_time) / TIME_DECAY_SNIPER_FEE;
            let dynamic_trade_fee_rate = FEE_RATE_DENOMINATOR_VALUE - fee_reduction as u32;
            (dynamic_trade_fee_rate, dynamic_trade_fee_rate, 0)
        };

    Ok((
        dynamic_trade_fee_rate,
        dynamic_protocol_fee_rate,
        dynamic_fund_fee_rate,
    ))
}
