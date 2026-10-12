use anchor_lang::prelude::*;

pub const SUPPORT_HOOK_SEED: &str = "support_hook";

/// An enshrined Token-2022 transfer-hook program: reviewed by the admin and proven immutable at enshrinement (no upgrade
/// authority, or a non-upgradeable loader), so what was reviewed is what runs. Mints whose active hook points at an
/// enshrined program may create pools and be used as rewards; their transfers carry the hook's extra accounts as
/// remaining accounts.
#[account]
#[derive(Default, Debug)]
pub struct SupportHookProgram {
    /// Bump to identify PDA
    pub bump: u8,
    /// Address of the enshrined transfer-hook program
    pub program_id: Pubkey,
    pub padding: [u64; 8],
}

impl SupportHookProgram {
    pub const LEN: usize = 8 + 1 + 32 + 64;
}
