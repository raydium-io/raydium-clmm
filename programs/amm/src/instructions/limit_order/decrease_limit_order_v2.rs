use crate::instructions::limit_order::decrease_limit_order::{
    decrease_limit_order_internal, DecreaseLimitOrder,
};
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `decrease_limit_order` with a declared account tape.
///
/// Both legs can move here: the settled output, and the refund of the unfilled input.
pub fn decrease_limit_order_v2<'info>(
    ctx: Context<'info, DecreaseLimitOrder<'info>>,
    amount: u64,
    amount_min: u64,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TickArrayBitmapExtension,
            AccountsType::TransferHookInput,
            AccountsType::TransferHookOutput,
        ],
    )?;
    let bitmap_extension = parsed.bitmap_extension();

    decrease_limit_order_internal(
        ctx,
        bitmap_extension,
        parsed.hook_input,
        parsed.hook_output,
        amount,
        amount_min,
    )
}
