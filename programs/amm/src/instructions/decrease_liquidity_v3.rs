use crate::instructions::decrease_liquidity::decrease_liquidity;
use crate::instructions::decrease_liquidity_v2::DecreaseLiquidityV2;
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `decrease_liquidity_v3` with a declared account tape, and **without** the reward payout.
///
/// Rewards still accrue — burning liquidity has to settle `reward_amount_owed` — but
/// collecting them moves to `collect_reward`. Bundling both would need 16 named accounts +
/// bitmap + 3 reward triples + 5 hook legs ≈ 66 accounts, over the 64 account-lock limit,
/// so the bundled version is not merely expensive but impossible to build.
///
/// `RewardGroup` on the tape is rejected rather than ignored, so a client that still thinks
/// this instruction pays rewards finds out immediately.
pub fn decrease_liquidity_v3<'info>(
    ctx: Context<'info, DecreaseLiquidityV2<'info>>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;

    decrease_liquidity(
        &ctx.accounts.pool_state,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &ctx.accounts.recipient_token_account_0.to_account_info(),
        &ctx.accounts.recipient_token_account_1.to_account_info(),
        &ctx.accounts.token_program,
        Some(ctx.accounts.token_program_2022.clone()),
        Some(ctx.accounts.memo_program.clone()),
        Some(ctx.accounts.vault_0_mint.clone()),
        Some(ctx.accounts.vault_1_mint.clone()),
        parsed.bitmap_extension(),
        None,
        parsed.hook_token_0,
        parsed.hook_token_1,
        liquidity,
        amount_0_min,
        amount_1_min,
    )
}
