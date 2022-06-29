use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetNewOwner<'info> {
    /// Current protocol owner
    // #[account(mut)]
    pub owner: Signer<'info>,
    /// Address to be designated as new protocol owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub new_owner: UncheckedAccount<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}

pub fn set_new_owner(ctx: Context<SetNewOwner>) -> Result<()> {
    let amm_config = &mut ctx.accounts.amm_config;
    require!(
        ctx.accounts.owner.key() == amm_config.owner
            || ctx.accounts.owner.key() == crate::admin::ID,
        ErrorCode::NotApproved
    );

    amm_config.owner = ctx.accounts.new_owner.key();

    emit!(OwnerChangedEvent {
        old_owner: ctx.accounts.owner.key(),
        new_owner: ctx.accounts.new_owner.key(),
    });

    Ok(())
}
