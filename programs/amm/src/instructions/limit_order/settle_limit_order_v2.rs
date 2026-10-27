use crate::instructions::limit_order::settle_limit_order::{
    settle_limit_order_internal, SettleLimitOrder,
};
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `settle_limit_order` with a declared account tape. Only the output leg transfers.
pub fn settle_limit_order_v2<'info>(
    ctx: Context<'info, SettleLimitOrder<'info>>,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;
    settle_limit_order_internal(ctx, parsed.hook_output)
}
