use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateAmmConfig<'info> {
    /// The amm config owner or admin
    #[account(address = crate::admin::id() @ ErrorCode::NotApproved)]
    pub owner: Signer<'info>,

    /// Amm config account to be changed
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}

pub fn update_amm_config(ctx: Context<UpdateAmmConfig>, param: u8, value: u32) -> Result<()> {
    let amm_config = &mut ctx.accounts.amm_config;
    let match_param = Some(param);
    match match_param {
        Some(0) => update_trade_fee_rate(amm_config, value),
        Some(1) => update_protocol_fee_rate(amm_config, value),
        Some(2) => update_fund_fee_rate(amm_config, value),
        Some(3) => {
            let new_owner = *ctx.remaining_accounts.iter().next().unwrap().key;
            set_new_owner(amm_config, new_owner);
        }
        Some(4) => {
            let new_fund_owner = *ctx.remaining_accounts.iter().next().unwrap().key;
            set_new_fund_owner(amm_config, new_fund_owner);
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }

    emit!(ConfigChangeEvent {
        index: amm_config.index,
        owner: amm_config.owner,
        trade_fee_rate: amm_config.trade_fee_rate,
        protocol_fee_rate: amm_config.protocol_fee_rate,
        tick_spacing: amm_config.tick_spacing,
        fund_fee_rate: amm_config.fund_fee_rate,
        fund_owner: amm_config.fund_owner,
    });

    Ok(())
}

fn update_protocol_fee_rate(amm_config: &mut Account<AmmConfig>, protocol_fee_rate: u32) {
    assert!(protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    assert!(protocol_fee_rate + amm_config.fund_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    amm_config.protocol_fee_rate = protocol_fee_rate;
}

fn update_trade_fee_rate(amm_config: &mut Account<AmmConfig>, trade_fee_rate: u32) {
    assert!(trade_fee_rate < FEE_RATE_DENOMINATOR_VALUE);
    amm_config.trade_fee_rate = trade_fee_rate;
}

fn update_fund_fee_rate(amm_config: &mut Account<AmmConfig>, fund_fee_rate: u32) {
    assert!(fund_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    assert!(fund_fee_rate + amm_config.protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
    amm_config.fund_fee_rate = fund_fee_rate;
}

fn set_new_owner(amm_config: &mut Account<AmmConfig>, new_owner: Pubkey) {
    #[cfg(feature = "enable-log")]
    msg!(
        "amm_config, old_owner:{}, new_owner:{}",
        amm_config.owner.to_string(),
        new_owner.key().to_string()
    );
    amm_config.owner = new_owner;
}

fn set_new_fund_owner(amm_config: &mut Account<AmmConfig>, new_fund_owner: Pubkey) {
    #[cfg(feature = "enable-log")]
    msg!(
        "amm_config, old_fund_owner:{}, new_fund_owner:{}",
        amm_config.fund_owner.to_string(),
        new_fund_owner.key().to_string()
    );
    amm_config.fund_owner = new_fund_owner;
}
