use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetProtocolFeeRate<'info> {
    /// Valid protocol owner
    #[account(address = amm_config.owner)]
    pub owner: Signer<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}
