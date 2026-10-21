//! Token-2022 TransferHook support.
//!
//! Any hook mint is admitted; there is no whitelist and its `ExtraAccountMetaList` is not
//! checked. `TransferHook.authority` and the hook program's upgrade authority are unchecked too:
//! both can change after the pool exists.

use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    self,
    extension::{transfer_hook, StateWithExtensions},
};

/// The hook program a mint points at, or `None`. The extension being present is not the
/// same as having a hook: `program_id` is an `OptionalNonZeroPubkey` and may be unset.
pub fn hook_program_id(mint_info: &AccountInfo) -> Result<Option<Pubkey>> {
    if *mint_info.owner != spl_token_2022::id() {
        return Ok(None);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    Ok(transfer_hook::get_program_id(&mint))
}

/// Reject a mint that points at a transfer hook program.
///
/// Used by the legacy (pre-v3) instruction entry points: their account layouts have no
/// room for the hook's accounts, so the transfer would fail deep inside token-2022 with an
/// opaque error. Fail early with a name that says what to do instead.
pub fn reject_hook_mint(mint_info: &AccountInfo) -> Result<()> {
    require!(
        hook_program_id(mint_info)?.is_none(),
        ErrorCode::UnsupportedTransferHookMint
    );
    Ok(())
}
