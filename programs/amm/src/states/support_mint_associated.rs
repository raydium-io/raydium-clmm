use anchor_lang::prelude::*;

pub const SUPPORT_MINT_SEED: &str = "support_mint";

/// Holds the current owner of the factory
#[account]
#[derive(Default, Debug)]
pub struct SupportMintAssociated {
    /// Bump to identify PDA
    pub bump: u8,
    /// Address of the supported token22 mint
    pub mint: Pubkey,
    pub padding: [u64; 8],
}

impl SupportMintAssociated {
    pub const LEN: usize = 8 + 1 + 32 + 64;

    pub fn initialize<'info>(&mut self, bump: u8, mint: Pubkey) -> Result<()> {
        self.bump = bump;
        self.mint = mint;
        Ok(())
    }
}
