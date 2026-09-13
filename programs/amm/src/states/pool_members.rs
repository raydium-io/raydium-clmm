use anchor_lang::prelude::*;

pub const POOL_MEMBERS_SEED: &str = "pool_members";
pub const MEMBER_VAULT_SEED: &str = "member_vault";
pub const MAX_POOL_MEMBERS: usize = 8;

/// One constituent of a collection pool. Index 0 is always the pair's own base token (the pool
/// token that is not the collection's quote mint) and uses the pool's existing vault and fee fields.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct PoolMember {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub decimals: u8,
    pub padding0: [u8; 7],
    /// Fees owed to the protocol / fund in this member's token (unused for index 0, which is
    /// accounted on `PoolState`)
    pub protocol_fees_owed: u64,
    pub fund_fees_owed: u64,
    pub padding: [u64; 2],
}
impl PoolMember {
    pub const LEN: usize = 32 + 32 + 1 + 7 + 8 + 8 + 16;
}

/// Turns a CLMM pair into a collection pool: extra member vaults hang off the pool and any two
/// members (the base token included) swap directly against each other inside the pool. Intra swaps
/// are priced by a StableSwap over the members' rate-normalised balances, so the price is exactly
/// the collection rate when the pool is balanced and bends away from it as the pool skews.
/// The pair's tick curve against the quote mint is untouched.
#[account]
#[derive(Default, Debug)]
pub struct PoolMembers {
    pub bump: u8,
    /// Whether the base member is `token_0` (else `token_1`) of the pair
    pub base_is_token_0: bool,
    pub n: u8,
    pub padding0: [u8; 5],
    pub pool: Pubkey,
    pub collection: Pubkey,
    /// StableSwap amplification for intra swaps
    pub amp: u64,
    pub members: [PoolMember; MAX_POOL_MEMBERS],
    pub padding: [u64; 8],
}
impl PoolMembers {
    pub const LEN: usize = 8 + 1 + 1 + 1 + 5 + 32 + 32 + 8 + PoolMember::LEN * MAX_POOL_MEMBERS + 8 * 8;
    pub fn find(&self, mint: &Pubkey) -> Option<usize> {
        self.members[..self.n as usize].iter().position(|m| m.mint == *mint)
    }
}
