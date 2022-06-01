use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetNewOwner<'info> {
    /// Current protocol owner
    #[account(address = factory_state.owner)]
    pub owner: Signer<'info>,

    /// Address to be designated as new protocol owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub new_owner: UncheckedAccount<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub factory_state: Account<'info, FactoryState>,
}

pub fn set_new_owner(ctx: Context<SetNewOwner>) -> Result<()> {
    let factory_state = &mut ctx.accounts.factory_state;
    factory_state.owner = ctx.accounts.new_owner.key();

    emit!(OwnerChangedEvent {
        old_owner: ctx.accounts.owner.key(),
        new_owner: ctx.accounts.new_owner.key(),
    });

    Ok(())
}
