use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use std::ops::DerefMut;

#[derive(Accounts)]
pub struct ResetSqrtPrice<'info> {
    /// Address paying to create the pool
    #[account(address = crate::admin::id())]
    pub owner: Signer<'info>,
    /// Which config the pool belongs to
    #[account(address = pool_state.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,
    /// Initialize an account to store the pool state
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,
    /// Token_0 vault
    pub token_vault_0: Box<Account<'info, TokenAccount>>,
    /// Token_1 vault
    pub token_vault_1: Box<Account<'info, TokenAccount>>,
}

pub fn reset_sqrt_price(ctx: Context<ResetSqrtPrice>, sqrt_price_x64: u128) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.deref_mut();

    if ctx.accounts.token_vault_0.amount > 0 || ctx.accounts.token_vault_1.amount > 0 {
        return err!(ErrorCode::NotApproved);
    }
    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64)?;
    pool_state.sqrt_price_x64 = sqrt_price_x64;
    pool_state.tick_current = tick;

    Ok(())
}
