use anchor_lang::prelude::*;
pub const LOCKED_POSITION_SEED: &str = "locked_position";

#[account]
#[derive(Default, Debug)]
pub struct LockedPositionState {
    /// Bump to identify PDA
    pub bump: [u8; 1],
    /// Record owner
    pub owner: Pubkey,
    /// The ID of the pool with which this record is connected
    pub pool_id: Pubkey,
    /// The ID of the position with which this record is connected
    pub position_id: Pubkey,
    /// NFT Account
    pub nft_account: Pubkey,
    /// account update recent epoch
    pub recent_epoch: u64,
    /// Unused bytes for future upgrades.
    pub padding: [u64; 8],
}

impl LockedPositionState {
    pub const LEN: usize = 8 + 1 + 32 + 32 + 32 + 32 + 8 + 8 * 8;

    pub fn key(position_id: Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[LOCKED_POSITION_SEED.as_bytes(), position_id.as_ref()],
            &crate::id(),
        )
        .0
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        owner: Pubkey,
        pool_id: Pubkey,
        position_id: Pubkey,
        nft_account: Pubkey,
        recent_epoch:u64,
    ) {
        self.bump = [bump];
        self.owner = owner;
        self.pool_id = pool_id;
        self.position_id = position_id;
        self.nft_account = nft_account;
        self.recent_epoch = recent_epoch;
        self.padding = [0; 8];
    }

    pub fn check(&self, owner: Pubkey, position_id: Pubkey, nft_account: Pubkey) -> Result<()> {
        require_keys_eq!(self.owner, owner);
        require_keys_eq!(self.position_id, position_id);
        require_keys_eq!(self.nft_account, nft_account);
        Ok(())
    }
}
