//! Token collections: admin-defined rulesets, permissionless collections bound to a ruleset, and
//! rule-checked memberships. Pools whose two mints are members of one collection gain the discounted
//! `rebalance_swap_v2` path. Mirrors the CP-Swap module so each program is self-contained.
use crate::error::ErrorCode;
use anchor_lang::prelude::*;

pub const RULESET_SEED: &str = "ruleset";
pub const TOKEN_COLLECTION_SEED: &str = "token_collection";
pub const COLLECTION_MEMBER_SEED: &str = "collection_member";

/// Rates are expressed with 1e9 == 1.0 (a member's value in the collection's numeraire).
pub const RATE_ONE: u64 = 1_000_000_000;
/// Within a ruleset, members are 1:1 with the quote mint and with each other by default: a stable is a
/// stable, an LST is an LST. The rate only defines what "balanced" means for the rebalance fee gate;
/// every trade is still priced by the tick curve, so a 1:1 default can never drain a pool at par.
/// The collection authority may override a member's rate (e.g. an LST's exchange rate).
pub const DEFAULT_MEMBER_RATE: u64 = RATE_ONE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RuleKind {
    /// Any mint may join.
    Any = 0,
    /// Mint was launched through pump.fun: the supplied bonding curve must be the `["bonding-curve", mint]`
    /// PDA of `program_id`, owned by it, carry the Anchor `BondingCurve` discriminator and, unless
    /// `FLAG_ALLOW_MAYHEM`, not be a mayhem-mode coin.
    PumpFunLaunch = 1,
    /// Mint has no mint authority and no freeze authority.
    ImmutableMint = 2,
    /// Mint is the pool mint of an SPL stake pool (or a layout-compatible fork) owned by `program_id`.
    /// The member rate is the pool's exchange rate and can be re-read with `sync_member_rate`.
    Lst = 3,
    /// Mint is the base token of a Meteora dynamic-bonding-curve launch whose `PoolConfig.fee_claimer`
    /// is the ruleset's `program_id` field (our launchpad partner PDA). Proof: `[virtual_pool, pool_config]`.
    LaunchpadDbc = 4,
}
impl RuleKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Any),
            1 => Some(Self::PumpFunLaunch),
            2 => Some(Self::ImmutableMint),
            3 => Some(Self::Lst),
            4 => Some(Self::LaunchpadDbc),
            _ => None,
        }
    }
}
pub const FLAG_ALLOW_MAYHEM: u8 = 1 << 0;
pub const FLAG_REQUIRE_COMPLETE: u8 = 1 << 1;

/// Admin-defined admission rule. Creation and updates are gated to `crate::admin::ID`;
/// referencing a ruleset from a collection is permissionless.
#[account]
#[derive(Default, Debug)]
pub struct Ruleset {
    pub bump: u8,
    pub index: u16,
    pub kind: u8,
    pub flags: u8,
    pub padding0: [u8; 3],
    /// Program whose accounts act as proof (e.g. pump.fun for `PumpFunLaunch`)
    pub program_id: Pubkey,
    pub padding: [u64; 8],
}
impl Ruleset {
    pub const LEN: usize = 8 + 1 + 2 + 1 + 1 + 3 + 32 + 8 * 8;
}

/// Permissionless whitelist of mints that satisfy one ruleset.
#[account]
#[derive(Default, Debug)]
pub struct TokenCollection {
    pub bump: u8,
    pub index: u16,
    pub padding0: [u8; 5],
    /// Creator; may update the fee divisor and member rates
    pub authority: Pubkey,
    /// Ruleset every member must satisfy
    pub ruleset: Pubkey,
    /// Numeraire of the collection (e.g. WSOL); always admitted as a member without a rule check
    pub quote_mint: Pubkey,
    /// Rebalance swaps pay `amm_config.trade_fee_rate / rebalance_fee_divisor`
    pub rebalance_fee_divisor: u32,
    pub member_count: u32,
    /// Second mint admitted without a rule check (Pubkey::default() = none). Lets a collection pool
    /// be anchored on a pair the rule would not admit, e.g. protocol-token/quote for a launchpad
    /// collection whose rule is "launched on our bonding curve".
    pub anchor_mint: Pubkey,
    pub padding: [u64; 4],
}
impl TokenCollection {
    pub const LEN: usize = 8 + 1 + 2 + 5 + 32 + 32 + 32 + 4 + 4 + 32 + 8 * 4;
}

/// Proof that `mint` satisfied the collection's ruleset at registration time.
#[account]
#[derive(Default, Debug)]
pub struct CollectionMember {
    pub bump: u8,
    pub padding0: [u8; 7],
    pub collection: Pubkey,
    pub mint: Pubkey,
    /// Value of one whole token in the collection's numeraire, 1e9 == 1.0.
    /// Defaults to `DEFAULT_MEMBER_RATE` (1:1 within the ruleset).
    pub rate: u64,
    pub registered_by: Pubkey,
    pub padding: [u64; 8],
}
impl CollectionMember {
    pub const LEN: usize = 8 + 1 + 7 + 32 + 32 + 8 + 32 + 8 * 8;
}

/// sqrt(price_1_per_0) in Q64.64 for the balanced state implied by the members' rates:
/// price = (rate_0 / rate_1) * 10^(decimals_1 - decimals_0)
pub fn target_sqrt_price_x64(rate_0: u64, rate_1: u64, decimals_0: u8, decimals_1: u8) -> Result<u128> {
    let num = u128::from(rate_0).checked_mul(10u128.pow(u32::from(decimals_1))).ok_or(ErrorCode::CalculateOverflow)?;
    let den = u128::from(rate_1).checked_mul(10u128.pow(u32::from(decimals_0))).ok_or(ErrorCode::CalculateOverflow)?;
    require!(num.leading_zeros() >= 64 && den > 0, ErrorCode::CalculateOverflow);
    let q = (num << 64) / den; // price * 2^64
    // sqrt(price * 2^64) * 2^32 == sqrt(price) * 2^64
    Ok(isqrt(q) << 32)
}
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << ((128 - n.leading_zeros() + 1) / 2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_price_peg() {
        assert_eq!(target_sqrt_price_x64(RATE_ONE, RATE_ONE, 9, 9).unwrap(), 1u128 << 64);
        let s = target_sqrt_price_x64(RATE_ONE, RATE_ONE / 150, 9, 6).unwrap();
        let price = (s as f64 / 2f64.powi(64)).powi(2);
        assert!((price - 0.15).abs() < 1e-6, "{price}");
    }
}
