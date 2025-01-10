use super::{create_pool, CreatePool};
use anchor_lang::prelude::*;

pub fn create_pool_v2(
    ctx: Context<CreatePool>,
    sqrt_price_x64: u128,
    open_time: u64,
    nonce: Option<u8>,
) -> Result<()> {
    create_pool(ctx, sqrt_price_x64, open_time, nonce)
}
