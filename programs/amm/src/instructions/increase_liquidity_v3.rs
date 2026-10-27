use crate::instructions::increase_liquidity::increase_liquidity;
use crate::instructions::increase_liquidity_v2::IncreaseLiquidityV2;
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `increase_liquidity_v2` with a declared account tape.
pub fn increase_liquidity_v3<'info>(
    ctx: Context<'info, IncreaseLiquidityV2<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    base_flag: Option<bool>,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;

    increase_liquidity(
        &ctx.accounts.nft_owner,
        &ctx.accounts.pool_state,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &ctx.accounts.token_account_0.to_account_info(),
        &ctx.accounts.token_account_1.to_account_info(),
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.token_program,
        Some(&ctx.accounts.token_program_2022),
        Some(ctx.accounts.vault_0_mint.clone()),
        Some(ctx.accounts.vault_1_mint.clone()),
        parsed.bitmap_extension(),
        parsed.hook_token_0,
        parsed.hook_token_1,
        liquidity,
        amount_0_max,
        amount_1_max,
        base_flag,
    )
}
