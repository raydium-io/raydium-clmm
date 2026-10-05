use crate::error::ErrorCode;
use crate::instructions::swap_v2::{exact_internal_v3, SwapSingleV2};
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `swap_v2` with a declared account tape.
///
/// The tick arrays no longer have to be identified by account size, and both transfer legs
/// get their own hook accounts. Account structure is `SwapSingleV2`, unchanged.
pub fn swap_v3<'info>(
    ctx: Context<'info, SwapSingleV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::TickArray,
            AccountsType::TickArrayBitmapExtension,
            AccountsType::TransferHookInput,
            AccountsType::TransferHookOutput,
        ],
    )?;

    let tick_arrays: Vec<&AccountInfo> = parsed.tick_array.iter().collect();
    let amount_result = exact_internal_v3(
        ctx.accounts,
        tick_arrays.as_slice(),
        parsed.bitmap_extension(),
        parsed.hook_input,
        parsed.hook_output,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
    )?;
    if is_base_input {
        require_gte!(
            amount_result,
            other_amount_threshold,
            ErrorCode::TooLittleOutputReceived
        );
    } else {
        require_gte!(
            other_amount_threshold,
            amount_result,
            ErrorCode::TooMuchInputPaid
        );
    }

    Ok(())
}
