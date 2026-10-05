use crate::instructions::limit_order::increase_limit_order::{
    increase_limit_order_internal, IncreaseLimitOrder,
};
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `increase_limit_order` with a declared account tape.
pub fn increase_limit_order_v2<'info>(
    ctx: Context<'info, IncreaseLimitOrder<'info>>,
    amount: u64,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TickArrayBitmapExtension,
            AccountsType::TransferHookInput,
        ],
    )?;
    let bitmap_extension = parsed.bitmap_extension();

    increase_limit_order_internal(ctx, bitmap_extension, parsed.hook_input, amount)
}
