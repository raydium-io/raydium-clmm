//! Ruleset evaluation. A rule may only read the mint and the proof accounts supplied by the
//! registrant, so membership is a pure function of on-chain state and anyone can register a mint.
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

/// sha256("account:BondingCurve")[..8]
pub const PUMP_BONDING_CURVE_DISCRIMINATOR: [u8; 8] = [23, 183, 248, 55, 96, 216, 172, 96];
pub const PUMP_BONDING_CURVE_SEED: &[u8] = b"bonding-curve";
/// pump.fun `BondingCurve` layout (after the 8-byte discriminator):
/// 5 x u64 reserves/supply, `complete: bool`, `creator: Pubkey`, `is_mayhem_mode: bool`, ...
const PUMP_COMPLETE_OFFSET: usize = 8 + 5 * 8;
const PUMP_IS_MAYHEM_OFFSET: usize = PUMP_COMPLETE_OFFSET + 1 + 32;

pub fn check_rules<'info>(
    ruleset: &Ruleset,
    mint: &InterfaceAccount<'info, Mint>,
    proof: &[AccountInfo<'info>],
) -> Result<()> {
    match RuleKind::from_u8(ruleset.kind).ok_or(ErrorCode::InvalidRuleKind)? {
        RuleKind::Any => Ok(()),
        RuleKind::ImmutableMint => {
            require!(
                mint.mint_authority.is_none() && mint.freeze_authority.is_none(),
                ErrorCode::RuleCheckFailed
            );
            Ok(())
        }
        RuleKind::PumpFunLaunch => check_pump_fun_launch(ruleset, &mint.key(), proof),
    }
}

/// 1. bonding curve is owned by the pump program and carries the Anchor `BondingCurve` type
/// 2. bonding curve is the `["bonding-curve", mint]` PDA of that program (so it belongs to this mint)
/// 3. not a mayhem-mode launch unless the ruleset allows it; optionally must have completed
fn check_pump_fun_launch(ruleset: &Ruleset, mint: &Pubkey, proof: &[AccountInfo]) -> Result<()> {
    let bonding_curve = proof.first().ok_or(ErrorCode::RuleCheckFailed)?;
    require_keys_eq!(*bonding_curve.owner, ruleset.program_id, ErrorCode::RuleCheckFailed);
    let (expected, _) = Pubkey::find_program_address(
        &[PUMP_BONDING_CURVE_SEED, mint.as_ref()],
        &ruleset.program_id,
    );
    require_keys_eq!(bonding_curve.key(), expected, ErrorCode::RuleCheckFailed);
    let data = bonding_curve.try_borrow_data()?;
    require!(
        data.len() > PUMP_COMPLETE_OFFSET && data[..8] == PUMP_BONDING_CURVE_DISCRIMINATOR,
        ErrorCode::RuleCheckFailed
    );
    // Curves created before `create_v2` predate the flag; a missing byte means not mayhem.
    let is_mayhem = data.get(PUMP_IS_MAYHEM_OFFSET).copied().unwrap_or(0) != 0;
    if ruleset.flags & FLAG_ALLOW_MAYHEM == 0 {
        require!(!is_mayhem, ErrorCode::RuleCheckFailed);
    }
    if ruleset.flags & FLAG_REQUIRE_COMPLETE != 0 {
        require!(data[PUMP_COMPLETE_OFFSET] != 0, ErrorCode::RuleCheckFailed);
    }
    Ok(())
}

pub fn validate_ruleset_params(kind: u8, flags: u8, program_id: &Pubkey) -> Result<()> {
    match RuleKind::from_u8(kind).ok_or(ErrorCode::InvalidRuleKind)? {
        RuleKind::PumpFunLaunch => {
            require_keys_neq!(*program_id, Pubkey::default(), ErrorCode::InvalidUpdateConfigFlag);
            require!(flags & !(FLAG_ALLOW_MAYHEM | FLAG_REQUIRE_COMPLETE) == 0, ErrorCode::InvalidUpdateConfigFlag);
        }
        _ => require_eq!(flags, 0, ErrorCode::InvalidUpdateConfigFlag),
    }
    Ok(())
}
