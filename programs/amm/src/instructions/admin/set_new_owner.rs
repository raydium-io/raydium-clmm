use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetNewOwner<'info> {
    /// Current amm config owner
    // #[account(mut)]
    pub owner: Signer<'info>,

    /// Address to be designated as new amm config owner
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub new_owner: UncheckedAccount<'info>,

    /// Amm config account to be changed
    #[account(mut)]
    pub amm_config: Account<'info, AmmConfig>,
}

pub fn set_new_owner(ctx: Context<SetNewOwner>) -> Result<()> {
    let amm_config = &mut ctx.accounts.amm_config;
    #[cfg(feature = "enable-log")]
    msg!(
        "amm_config.owner:{}, signer:{}, new_owner:{}",
        amm_config.owner.to_string(),
        ctx.accounts.owner.key().to_string(),
        ctx.accounts.new_owner.key().to_string()
    );
    require!(
        ctx.accounts.owner.key() == amm_config.owner
            || ctx.accounts.owner.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );

    amm_config.owner = ctx.accounts.new_owner.key();

    emit!(OwnerChangedEvent {
        old_owner: ctx.accounts.owner.key(),
        new_owner: ctx.accounts.new_owner.key(),
    });

    Ok(())
}
