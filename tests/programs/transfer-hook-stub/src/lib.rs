//! A Token-2022 transfer hook that does nothing, for tests. Without some hook program on the
//! validator a hook mint has no `ExtraAccountMetaList` and every transfer of it fails.
//!
//! Both instructions use the SPL transfer-hook interface discriminators rather than anchor's,
//! so they dispatch from `fallback`. `InitializeExtraAccountMetaList` uses `allocate` + `assign`
//! rather than `create_account` because the caller pre-funds the PDA - **real hook programs must
//! do the same**. `Execute` performs no logic of its own but checks its accounts the way the SPL
//! reference hook does, so a caller that drops the list or any extra fails here instead of
//! passing silently.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke_signed, system_instruction};
use spl_tlv_account_resolution::{account::ExtraAccountMeta, state::ExtraAccountMetaList};
use spl_transfer_hook_interface::instruction::{ExecuteInstruction, TransferHookInstruction};

// A fixed address with no keypair on purpose: this program is never deployed. The tests write
// its account and ProgramData straight into the surfnet at this address (see
// `installStubHookProgram`), which needs no signature from it. The address is not free-floating
// though - anchor's generated `entry` rejects a call whose program id differs from this line
// with `DeclaredProgramIdMismatch`, so tests must install it here and nowhere else.
declare_id!("6s7wXukDLKt39UKgbQVExCpwrAsRraEB8jkV6P95ZcSR");

pub const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";

#[program]
pub mod transfer_hook_stub {
    use super::*;

    pub fn fallback(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> Result<()> {
        match TransferHookInstruction::unpack(data)
            .map_err(|_| error!(StubError::UnknownInstruction))?
        {
            TransferHookInstruction::Execute { .. } => execute(program_id, accounts, data),
            TransferHookInstruction::InitializeExtraAccountMetaList {
                extra_account_metas,
            } => initialize_list(program_id, accounts, &extra_account_metas),
            _ => err!(StubError::UnknownInstruction),
        }
    }
}

/// Accounts, in the order the interface defines them:
/// `[source, mint, destination, authority, list PDA, ...extras]`
///
/// Without the list token-2022 still calls the hook, with just the first four accounts, so
/// its absence has to be rejected here. `check_account_infos` then requires every extra to sit
/// at its declared position with its declared privileges.
fn execute(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> Result<()> {
    let [_source, mint_info, _destination, _authority, list_info, ..] = accounts else {
        return err!(StubError::MissingExtraAccountMetaList);
    };
    let (expected, _) = Pubkey::find_program_address(
        &[EXTRA_ACCOUNT_METAS_SEED, mint_info.key.as_ref()],
        program_id,
    );
    require_keys_eq!(
        *list_info.key,
        expected,
        StubError::MissingExtraAccountMetaList
    );
    require_keys_eq!(
        *list_info.owner,
        *program_id,
        StubError::MissingExtraAccountMetaList
    );

    let list_data = list_info.try_borrow_data()?;
    ExtraAccountMetaList::check_account_infos::<ExecuteInstruction>(
        accounts, data, program_id, &list_data,
    )
    .map_err(|_| error!(StubError::IncorrectExtraAccounts))
}

/// Accounts, in the order the interface defines them:
/// `[list PDA (w), mint, authority (s), system program]`
fn initialize_list(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    metas: &[ExtraAccountMeta],
) -> Result<()> {
    let [list_info, mint_info, _authority_info, _system_program] = accounts else {
        return err!(StubError::NotEnoughAccounts);
    };

    let (expected, bump) = Pubkey::find_program_address(
        &[EXTRA_ACCOUNT_METAS_SEED, mint_info.key.as_ref()],
        program_id,
    );
    require_keys_eq!(*list_info.key, expected, StubError::WrongListAddress);

    let size = ExtraAccountMetaList::size_of(metas.len())
        .map_err(|_| error!(StubError::UnknownInstruction))?;
    let seeds: &[&[u8]] = &[EXTRA_ACCOUNT_METAS_SEED, mint_info.key.as_ref(), &[bump]];

    // `allocate` + `assign`, never `create_account`: the caller has already funded the PDA.
    invoke_signed(
        &system_instruction::allocate(list_info.key, size as u64),
        &[list_info.clone()],
        &[seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(list_info.key, program_id),
        &[list_info.clone()],
        &[seeds],
    )?;

    let mut data = list_info.try_borrow_mut_data()?;
    ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, metas)
        .map_err(|_| error!(StubError::UnknownInstruction))?;
    Ok(())
}

#[error_code]
pub enum StubError {
    #[msg("instruction is not on the transfer hook interface")]
    UnknownInstruction,
    #[msg("not enough accounts")]
    NotEnoughAccounts,
    #[msg("extra account meta list is not the pda of this mint")]
    WrongListAddress,
    #[msg("execute was called without this mint's extra account meta list")]
    MissingExtraAccountMetaList,
    #[msg("execute accounts do not match the extra account meta list")]
    IncorrectExtraAccounts,
}
