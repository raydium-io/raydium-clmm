use crate::states::*;
use crate::{error::ErrorCode, Result};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseLimitOrder<'info> {
    #[account(constraint = signer.key() == limit_order.owner || signer.key() == crate::limit_order_admin::ID)]
    pub signer: Signer<'info>,

    /// CHECK: The rent receiver account
    #[account(mut, address = limit_order.owner)]
    pub rent_receiver: UncheckedAccount<'info>,

    #[account(mut)]
    pub limit_order: Account<'info, LimitOrderState>,
}

pub fn close_limit_order<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CloseLimitOrder<'info>>,
) -> Result<()> {
    // Close accounts when fully unfilled amount becomes zero
    if ctx.accounts.limit_order.get_unfilled_amount()? != 0 {
        return err!(ErrorCode::InvalidLimitOrderAmount);
    }

    // close limit order account
    AccountsClose::close(
        &ctx.accounts.limit_order,
        ctx.accounts.rent_receiver.to_account_info(),
    )?;

    Ok(())
}
