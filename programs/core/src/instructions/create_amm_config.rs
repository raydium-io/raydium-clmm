use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::DerefMut;

#[derive(Accounts)]
pub struct CreateAmmConfig<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Initialize factory state account to store protocol owner address
    #[account(
        init,
        seeds = [],
        bump,
        payer = owner,
        space = AmmConfig::LEN
    )]
    pub amm_config: Account<'info, AmmConfig>,

    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn create_amm_config(ctx: Context<CreateAmmConfig>) -> Result<()> {
    let amm_config = ctx.accounts.amm_config.deref_mut();
    amm_config.bump = *ctx.bumps.get("amm_config").unwrap();
    amm_config.owner = ctx.accounts.owner.key();
    amm_config.protocol_fee = 3; // 1/3 = 33.33%

    emit!(CreateConfigEvent {
        owner: ctx.accounts.owner.key(),
        protocol_fee: amm_config.protocol_fee,
    });

    Ok(())
}
