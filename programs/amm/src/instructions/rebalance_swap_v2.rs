use crate::error::ErrorCode;
use crate::instructions::swap_v2::*;
use crate::states::*;
use anchor_lang::prelude::*;

/// `swap_v2` for a pool whose two mints are members of one token collection.
/// LP fee is `amm_config.trade_fee_rate / collection.rebalance_fee_divisor`; the swap must move
/// `sqrt_price_x64` strictly closer to the target implied by the members' rates.
#[derive(Accounts)]
pub struct RebalanceSwapSingleV2<'info> {
    pub swap: SwapSingleV2<'info>,

    #[account(
        seeds = [
            TOKEN_COLLECTION_SEED.as_bytes(),
            collection.authority.as_ref(),
            &collection.index.to_le_bytes()
        ],
        bump = collection.bump,
    )]
    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            collection.key().as_ref(),
            swap.input_vault_mint.key().as_ref()
        ],
        bump = input_member.bump,
        constraint = input_member.collection == collection.key() @ ErrorCode::InvalidCollectionMember,
    )]
    pub input_member: Box<Account<'info, CollectionMember>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            collection.key().as_ref(),
            swap.output_vault_mint.key().as_ref()
        ],
        bump = output_member.bump,
        constraint = output_member.collection == collection.key() @ ErrorCode::InvalidCollectionMember,
    )]
    pub output_member: Box<Account<'info, CollectionMember>>,
    // remaining accounts: same as swap_v2 (tick arrays, optional bitmap extension)
}

pub fn rebalance_swap_v2<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, RebalanceSwapSingleV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let divisor = ctx.accounts.collection.rebalance_fee_divisor;
    let (input_rate, output_rate) = (ctx.accounts.input_member.rate, ctx.accounts.output_member.rate);
    let swap = &mut ctx.accounts.swap;
    // Never below 1 ppm, mirroring CP-Swap.
    let fee_rate = swap
        .amm_config
        .trade_fee_rate
        .checked_div(divisor)
        .ok_or(ErrorCode::InvalidUpdateConfigFlag)?
        .max(1);

    let (zero_for_one, sqrt_before, target) = {
        let pool = swap.pool_state.load()?;
        let zero_for_one = swap.input_vault.mint == pool.token_mint_0;
        let (rate_0, rate_1) = if zero_for_one { (input_rate, output_rate) } else { (output_rate, input_rate) };
        (
            zero_for_one,
            pool.sqrt_price_x64,
            target_sqrt_price_x64(rate_0, rate_1, pool.mint_decimals_0, pool.mint_decimals_1)?,
        )
    };
    // A rebalance can only push the price toward the target.
    require!(
        if zero_for_one { sqrt_before > target } else { sqrt_before < target },
        ErrorCode::NotRebalancing
    );

    let amount_result = exact_internal_v2_with_fee(
        swap,
        ctx.remaining_accounts,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
        fee_rate,
    )?;
    if is_base_input {
        require_gte!(amount_result, other_amount_threshold, ErrorCode::TooLittleOutputReceived);
    } else {
        require_gte!(other_amount_threshold, amount_result, ErrorCode::TooMuchInputPaid);
    }

    let sqrt_after = swap.pool_state.load()?.sqrt_price_x64;
    require_gt!(sqrt_before.abs_diff(target), sqrt_after.abs_diff(target), ErrorCode::NotRebalancing);
    Ok(())
}
