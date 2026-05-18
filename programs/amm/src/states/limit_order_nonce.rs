use anchor_lang::prelude::*;

#[account]
#[derive(Default)]
pub struct LimitOrderNonce {
    pub user_wallet: Pubkey,
    pub nonce_index: u8,
    // The next nonce, used to create user's limit order PDA account.
    pub order_nonce: u64,
    pub padding: [u64; 4],
}

impl LimitOrderNonce {
    pub const LEN: usize = 8 + 32 + 1 + 8 + 8 * 4;

    pub fn increase_order_nonce(&mut self) -> Result<()> {
        // Use u64::MAX(18446744073709551615) is enough to represent the order nonce for single user.
        // If a user opens 100 orders per second, it would take 5849424173.55072(18446744073709551615 / (3600 * 24 * 365 * 100)) years to reach the maximum value.
        self.order_nonce = self.order_nonce.saturating_add(1);
        return Ok(());
    }
}
