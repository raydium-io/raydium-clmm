use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateAmmConfig<'info> {
    /// The amm config owner or admin
    pub owner: Signer<'info>,

    /// Amm config account to be changed
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}

pub fn update_amm_config(
    ctx: Context<UpdateAmmConfig>,
    new_owner: Pubkey,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    flag: u8,
) -> Result<()> {
    require!(
        ctx.accounts.owner.key() == ctx.accounts.amm_config.owner
            || ctx.accounts.owner.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );
    let amm_config = &mut ctx.accounts.amm_config;
    let match_flag = Some(flag);
    match match_flag {
        Some(0) => set_new_owner(amm_config, new_owner),
        Some(1) => update_trade_fee_rate(amm_config, trade_fee_rate),
        Some(2) => update_protocol_fee_rate(amm_config, protocol_fee_rate),
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }

    emit!(UpdaterConfigEvent {
        owner: amm_config.owner,
        trade_fee_rate: amm_config.trade_fee_rate,
        protocol_fee_rate: amm_config.protocol_fee_rate
    });

    Ok(())
}

fn update_protocol_fee_rate(amm_config: &mut Account<AmmConfig>, protocol_fee_rate: u32) {
    assert!(protocol_fee_rate > 0 && protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    amm_config.protocol_fee_rate = protocol_fee_rate;
}

fn update_trade_fee_rate(amm_config: &mut Account<AmmConfig>, trade_fee_rate: u32) {
    assert!(trade_fee_rate < FEE_RATE_DENOMINATOR_VALUE); 
    amm_config.trade_fee_rate = trade_fee_rate;
}

fn set_new_owner(amm_config: &mut Account<AmmConfig>, new_owner: Pubkey) {
    #[cfg(feature = "enable-log")]
    msg!(
        "amm_config.owner:{}, signer:{}, new_owner:{}",
        amm_config.owner.to_string(),
        ctx.accounts.owner.key().to_string(),
        ctx.accounts.new_owner.key().to_string()
    );
    amm_config.owner = new_owner;
}
