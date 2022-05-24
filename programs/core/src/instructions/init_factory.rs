use crate::states::*;
use anchor_lang::prelude::*;
use std::{mem::size_of, ops::DerefMut};

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
        space = 8 + size_of::<FactoryState>()
    )]
    pub factory_state: Account<'info, FactoryState>,

    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn init_factory(ctx: Context<Initialize>) -> Result<()> {
    let factory_state = ctx.accounts.factory_state.deref_mut();
    factory_state.bump = *ctx.bumps.get("factory_state").unwrap();
    factory_state.owner = ctx.accounts.owner.key();
    factory_state.fee_protocol = 3; // 1/3 = 33.33%

    emit!(OwnerChanged {
        old_owner: Pubkey::default(),
        new_owner: ctx.accounts.owner.key(),
    });

    Ok(())
}
