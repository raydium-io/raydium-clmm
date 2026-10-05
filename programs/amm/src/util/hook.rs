//! Token-2022 TransferHook support.
//!
//! These checks guarantee a hook mint can be *traded*, not that it is safe. Admission on CLMM is
//! the `SupportMintAssociated` whitelist and nothing else.
//!
//! Deliberately unchecked: `TransferHook.authority` and the hook program's upgrade authority.
//! Both can change after the pool exists; that risk is accepted and handled by operations.

use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    self,
    extension::{transfer_hook, StateWithExtensions},
};
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, pubkey_data::PubkeyData, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;
use spl_type_length_value::state::TlvStateBorrowed;

/// Upper bound on the extra accounts a hook may declare. The protocol hard wall is 15
/// (token-2022 OOMs at 16); 6 leaves head room for multi-hop routes against the 64 account-lock
/// limit. Checked only at pool creation: a hook that grows afterwards must not lock LPs in.
pub const MAX_HOOK_EXTRA_ACCOUNTS: usize = 6;

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

/// A `TransferHook` extension that can never trigger a CPI: no program now, and no authority
/// to set one later. With an authority present, the hook can be switched on after the pool
/// exists, so such a mint must go through the whitelist like any active hook mint.
pub fn transfer_hook_is_inert(ext: &transfer_hook::TransferHook) -> bool {
    Option::<Pubkey>::from(ext.program_id).is_none()
        && Option::<Pubkey>::from(ext.authority).is_none()
}

/// Locate an account by key in a slice.
fn find_account<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
    key: &Pubkey,
) -> Option<&'a AccountInfo<'info>> {
    accounts.iter().find(|a| a.key == key)
}

/// Pool-creation gate. A no-op for mints without a hook, so callers invoke it unconditionally.
/// `remaining_accounts` only needs to contain the mint's `ExtraAccountMetaList` PDA.
pub fn validate_hook_mint(
    mint_info: &AccountInfo,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    let Some(program_id) = hook_program_id(mint_info)? else {
        return Ok(());
    };
    validate_extra_account_meta_list(mint_info.key, &program_id, remaining_accounts)
}

/// Parse the mint's `ExtraAccountMetaList` and reject anything we cannot build a
/// transaction for.
///
/// Split out from [`validate_hook_mint`] so a caller that just created the list via CPI can
/// validate it directly, without re-reading the mint extension.
fn validate_extra_account_meta_list(
    mint: &Pubkey,
    program_id: &Pubkey,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    let (metas_addr, _) =
        Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], program_id);
    let metas_info = find_account(remaining_accounts, &metas_addr)
        .ok_or(ErrorCode::NoExtraAccountsForTransferHook)?;
    require_keys_eq!(
        *metas_info.owner,
        *program_id,
        ErrorCode::HookProgramNotSupported
    );

    let data = metas_info.try_borrow_data()?;
    let tlv =
        TlvStateBorrowed::unpack(&data).map_err(|_| error!(ErrorCode::HookProgramNotSupported))?;
    let list = ExtraAccountMetaList::unpack_with_tlv_state::<ExecuteInstruction>(&tlv)
        .map_err(|_| error!(ErrorCode::HookProgramNotSupported))?;

    require_gte!(
        MAX_HOOK_EXTRA_ACCOUNTS,
        list.len(),
        ErrorCode::HookExtraAccountsTooMany
    );

    for meta in list.iter() {
        check_meta_resolvable(meta)?;
    }
    Ok(())
}

/// Reject an extra account whose address the AMM cannot resolve, since every transfer of the
/// mint would then fail.
///
/// Execute instruction data: the only thing in it is the transfer amount, and on an AMM the
/// amount of at least one leg is computed on-chain — so such a mint's account list cannot be
/// assembled off-chain at all. Resolving with a placeholder amount would just produce the
/// wrong accounts.
///
/// Source/destination account data: see [`check_account_data`].
///
/// `ExtraAccountMeta::discriminator` encoding:
///   0        literal pubkey
///   1        PDA derived from the hook program; `address_config` holds packed seeds
///   2        `PubkeyData`: address read from account data or instruction data
///   >= 128   PDA derived from the program at account index `discriminator - 128`; packed seeds
fn check_meta_resolvable(meta: &ExtraAccountMeta) -> Result<()> {
    const U8_TOP_BIT: u8 = 1 << 7;
    match meta.discriminator {
        0 => Ok(()),
        2 => match PubkeyData::unpack(&meta.address_config)
            .map_err(|_| error!(ErrorCode::HookProgramNotSupported))?
        {
            PubkeyData::InstructionData { .. } => {
                err!(ErrorCode::HookExtraAccountUsesInstructionData)
            }
            PubkeyData::AccountData {
                account_index,
                data_index,
            } => check_account_data(account_index, data_index, std::mem::size_of::<Pubkey>()),
            PubkeyData::Uninitialized => Ok(()),
        },
        d if d == 1 || d >= U8_TOP_BIT => {
            let seeds = Seed::unpack_address_config(&meta.address_config)
                .map_err(|_| error!(ErrorCode::HookProgramNotSupported))?;
            for seed in &seeds {
                match seed {
                    Seed::InstructionData { .. } => {
                        return err!(ErrorCode::HookExtraAccountUsesInstructionData)
                    }
                    Seed::AccountData {
                        account_index,
                        data_index,
                        length,
                    } => check_account_data(*account_index, *data_index, usize::from(*length))?,
                    _ => {}
                }
            }
            Ok(())
        }
        // Unknown discriminator: refuse rather than let a future upstream encoding through.
        _ => err!(ErrorCode::HookProgramNotSupported),
    }
}

/// An extra account derived from `length` bytes of Execute account `account_index`, starting at
/// `data_index`. Of the source (0) and destination (2) token accounts only `mint` and `owner`,
/// the first 64 bytes, are stable: past them sit the balance, the delegated amount and the
/// `TransferHookAccount.transferring` flag, all of which the transfer rewrites between the AMM
/// resolving the list before its CPI and token-2022 resolving it again afterwards.
fn check_account_data(account_index: u8, data_index: u8, length: usize) -> Result<()> {
    let transferred = account_index == 0 || account_index == 2;
    require!(
        !transferred || usize::from(data_index) + length <= 64,
        ErrorCode::HookExtraAccountUsesTransferredData
    );
    Ok(())
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
