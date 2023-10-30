use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::DerefMut;

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateAmmConfig<'info> {
    /// Address to be set as protocol owner.
    #[account(
        mut,
        address = crate::admin::id() @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    /// Initialize config state account to store protocol owner address and fee rates.
    #[account(
        init,
        seeds = [
            AMM_CONFIG_SEED.as_bytes(),
            &index.to_be_bytes()
        ],
        bump,
        payer = owner,
        space = AmmConfig::LEN
    )]
    pub amm_config: Account<'info, AmmConfig>,

    pub system_program: Program<'info, System>,
}

pub fn create_amm_config(
    ctx: Context<CreateAmmConfig>,
    index: u16,
    tick_spacing: u16,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    fund_fee_rate: u32,
) -> Result<()> {
    let amm_config = ctx.accounts.amm_config.deref_mut();
    amm_config.owner = ctx.accounts.owner.key();
    amm_config.bump = ctx.bumps.amm_config;
    amm_config.index = index;
    amm_config.trade_fee_rate = trade_fee_rate;
    amm_config.protocol_fee_rate = protocol_fee_rate;
    amm_config.tick_spacing = tick_spacing;
    amm_config.fund_fee_rate = fund_fee_rate;
    amm_config.fund_owner = ctx.accounts.owner.key();

    emit!(ConfigChangeEvent {
        index: amm_config.index,
        owner: ctx.accounts.owner.key(),
        protocol_fee_rate: amm_config.protocol_fee_rate,
        trade_fee_rate: amm_config.trade_fee_rate,
        tick_spacing: amm_config.tick_spacing,
        fund_fee_rate: amm_config.fund_fee_rate,
        fund_owner: amm_config.fund_owner,
    });

    Ok(())
}
