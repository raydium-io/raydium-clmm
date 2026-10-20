use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;
use std::ops::DerefMut;

#[derive(Accounts)]
pub struct CreateSupportHookProgram<'info> {
    /// Protocol admin, or the support-mint owner.
    #[account(
        mut,
        constraint = (owner.key() == crate::admin::ID || owner.key() == crate::create_support_mint_associated_owner::ID) @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,
    /// The transfer-hook program being enshrined. Must be executable and immutable: either deployed with a
    /// non-upgradeable loader, or an upgradeable program whose upgrade authority has been removed.
    /// CHECK: address, owner and executable flag are read; immutability is checked against `hook_program_data`.
    #[account(executable)]
    pub hook_program: UncheckedAccount<'info>,
    /// The upgradeable loader's ProgramData account for `hook_program`. Any account may be passed for a program
    /// owned by a non-upgradeable loader; it is ignored in that case.
    /// CHECK: validated in the handler against the loader-derived address and parsed for the upgrade authority.
    pub hook_program_data: UncheckedAccount<'info>,
    /// Records that `hook_program` is an approved, immutable transfer hook.
    #[account(
        init,
        seeds = [
            SUPPORT_HOOK_SEED.as_bytes(),
            hook_program.key().as_ref(),
        ],
        bump,
        payer = owner,
        space = SupportHookProgram::LEN
    )]
    pub support_hook_program: Account<'info, SupportHookProgram>,

    pub system_program: Program<'info, System>,
}

/// The non-upgradeable BPF loaders: a program they own can never change.
const BPF_LOADER_2: Pubkey = pubkey!("BPFLoader2111111111111111111111111111111111");
const BPF_LOADER_DEPRECATED: Pubkey = pubkey!("BPFLoader1111111111111111111111111111111111");

/// Offset of `upgrade_authority_address: Option<Pubkey>` inside `UpgradeableLoaderState::ProgramData`:
/// 4 bytes enum discriminant, 8 bytes `slot`, then the Option tag.
const PROGRAMDATA_AUTHORITY_TAG_OFFSET: usize = 12;
const PROGRAMDATA_DISCRIMINANT: u32 = 3;

/// Whether an upgradeable-loader ProgramData account has no upgrade authority (program is immutable).
pub fn programdata_is_immutable(data: &[u8]) -> bool {
    data.len() > PROGRAMDATA_AUTHORITY_TAG_OFFSET
        && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == PROGRAMDATA_DISCRIMINANT
        && data[PROGRAMDATA_AUTHORITY_TAG_OFFSET] == 0
}

pub fn create_support_hook_program(ctx: Context<CreateSupportHookProgram>) -> Result<()> {
    let program = &ctx.accounts.hook_program;
    let immutable = if *program.owner == bpf_loader_upgradeable::id() {
        // program account -> ProgramData PDA [program_id] under the upgradeable loader
        let (expected, _bump) =
            Pubkey::find_program_address(&[program.key().as_ref()], &bpf_loader_upgradeable::id());
        let pd = &ctx.accounts.hook_program_data;
        require_keys_eq!(pd.key(), expected, ErrorCode::HookProgramNotImmutable);
        require_keys_eq!(
            *pd.owner,
            bpf_loader_upgradeable::id(),
            ErrorCode::HookProgramNotImmutable
        );
        programdata_is_immutable(&pd.try_borrow_data()?)
    } else {
        // the non-upgradeable loaders can't change a deployed program
        *program.owner == BPF_LOADER_2 || *program.owner == BPF_LOADER_DEPRECATED
    };
    require!(immutable, ErrorCode::HookProgramNotImmutable);

    let state = ctx.accounts.support_hook_program.deref_mut();
    state.bump = ctx.bumps.support_hook_program;
    state.program_id = program.key();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programdata(authority: Option<[u8; 32]>) -> Vec<u8> {
        let mut d = 3u32.to_le_bytes().to_vec(); // ProgramData
        d.extend_from_slice(&7u64.to_le_bytes()); // slot
        match authority {
            None => d.push(0),
            Some(a) => {
                d.push(1);
                d.extend_from_slice(&a);
            }
        }
        d.extend_from_slice(&[0u8; 64]); // some bytecode
        d
    }

    #[test]
    fn no_upgrade_authority_is_immutable() {
        assert!(programdata_is_immutable(&programdata(None)));
    }

    #[test]
    fn upgrade_authority_present_is_mutable() {
        assert!(!programdata_is_immutable(&programdata(Some([9u8; 32]))));
    }

    #[test]
    fn wrong_discriminant_or_short_data_is_rejected() {
        let mut d = programdata(None);
        d[0] = 2; // Program, not ProgramData
        assert!(!programdata_is_immutable(&d));
        assert!(!programdata_is_immutable(&[3, 0, 0, 0]));
    }
}
