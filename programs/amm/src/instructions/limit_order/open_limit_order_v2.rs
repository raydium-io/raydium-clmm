use crate::instructions::limit_order::open_limit_order::{
    open_limit_order_internal, OpenLimitOrder,
};
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `open_limit_order` with a declared account tape. Only the input leg transfers.
pub fn open_limit_order_v2<'info>(
    ctx: Context<'info, OpenLimitOrder<'info>>,
    nonce_index: u8,
    zero_for_one: bool,
    tick_index: i32,
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

    open_limit_order_internal(
        ctx,
        bitmap_extension,
        parsed.hook_input,
        nonce_index,
        zero_for_one,
        tick_index,
        amount,
    )
}
