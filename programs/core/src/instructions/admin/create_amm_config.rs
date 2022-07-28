use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::DerefMut;

#[derive(Accounts)]
#[instruction(index: u8)]
pub struct CreateAmmConfig<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Initialize factory state account to store protocol owner address
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

    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn create_amm_config(
    ctx: Context<CreateAmmConfig>,
    index: u16,
    tick_spacing: u16,
    protocol_fee_rate: u32,
    trade_fee_rate: u32,
) -> Result<()> {
    let amm_config = ctx.accounts.amm_config.deref_mut();
    amm_config.owner = crate::admin::id();
    amm_config.bump = *ctx.bumps.get("amm_config").unwrap();
    amm_config.index = index;
    amm_config.protocol_fee_rate = protocol_fee_rate;
    amm_config.trade_fee_rate = trade_fee_rate;
    amm_config.tick_spacing = tick_spacing;

    emit!(CreateConfigEvent {
        owner: ctx.accounts.owner.key(),
        protocol_fee_rate: amm_config.protocol_fee_rate,
    });

    Ok(())
}
