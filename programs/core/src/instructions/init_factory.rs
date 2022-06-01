use crate::states::*;
use anchor_lang::prelude::*;
use std::{ops::DerefMut};

#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Address to be set as protocol owner. It pays to create factory state account.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Initialize factory state account to store protocol owner address
    #[account(
        init,
        seeds = [],
        bump,
        payer = owner,
        space = FactoryState::LEN
    )]
    pub factory_state: Account<'info, FactoryState>,

    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn init_factory(ctx: Context<Initialize>) -> Result<()> {
    let factory_state = ctx.accounts.factory_state.deref_mut();
    factory_state.bump = *ctx.bumps.get("factory_state").unwrap();
    factory_state.owner = ctx.accounts.owner.key();
    factory_state.protocol_fee = 3; // 1/3 = 33.33%

    emit!(InitFactoryEvent {
        owner: ctx.accounts.owner.key(),
        protocol_fee: factory_state.protocol_fee,
    });

    Ok(())
}
