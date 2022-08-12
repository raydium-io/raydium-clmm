use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetProtocolFeeRate<'info> {
    /// The amm config owner or admin
    pub owner: Signer<'info>,

    /// Amm config account to be changed
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}

pub fn set_protocol_fee_rate(
    ctx: Context<SetProtocolFeeRate>,
    protocol_fee_rate: u32,
) -> Result<()> {
    require!(
        ctx.accounts.owner.key() == ctx.accounts.amm_config.owner
            || ctx.accounts.owner.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );

    assert!(protocol_fee_rate > 0 && protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    
    let amm_config = &mut ctx.accounts.amm_config;
    let protocol_fee_rate_old = amm_config.protocol_fee_rate;
    amm_config.protocol_fee_rate = protocol_fee_rate;

    emit!(SetProtocolFeeRateEvent {
        protocol_fee_rate_old,
        protocol_fee_rate_new: protocol_fee_rate
    });

    Ok(())
}
