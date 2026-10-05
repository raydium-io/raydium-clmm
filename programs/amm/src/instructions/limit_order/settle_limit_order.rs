use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct SettleLimitOrder<'info> {
    #[account(constraint = signer.key() == limit_order.owner || signer.key() == crate::limit_order_admin::ID)]
    pub signer: Signer<'info>,

    #[account()]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        constraint = tick_array.load()?.pool_id == pool_state.key(),
    )]
    pub tick_array: AccountLoader<'info, TickArrayState>,

    #[account(
        mut,
        constraint = limit_order.pool_id == pool_state.key(),
    )]
    pub limit_order: Account<'info, LimitOrderState>,

    /// The owner's output token account (opposite direction from input)
    #[account(
        mut,
        token::mint = output_vault.mint,
        token::authority = limit_order.owner,
    )]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds output tokens
    #[account(
        mut,
        constraint = if limit_order.zero_for_one {
            output_vault.key() == pool_state.load()?.token_vault_1
        } else {
            output_vault.key() == pool_state.load()?.token_vault_0
        }
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint of output vault
    #[account(
        address = output_vault.mint
    )]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL-TOKEN / SPL-TOKEN2022 program for token transfers
    #[account(
        address = *output_vault_mint.to_account_info().owner
    )]
    pub output_token_program: Interface<'info, TokenInterface>,
    // remaining accounts:
    //   settle_limit_order: none
    //   settle_limit_order_v2: segments declared by `RemainingAccountsInfo`: TransferHookOutput
}

pub fn settle_limit_order<'info>(ctx: Context<'info, SettleLimitOrder<'info>>) -> Result<()> {
    // v1 has no way to carry the hook accounts, and it does move pool tokens
    // (`output_vault` -> `output_token_account`), so a hook output mint would otherwise fail
    // deep inside token-2022 with nothing pointing back here. Use `settle_limit_order_v2`.
    crate::util::hook::reject_hook_mint(&ctx.accounts.output_vault_mint.to_account_info())?;
    settle_limit_order_internal(ctx, &[])
}

pub fn settle_limit_order_internal<'info>(
    ctx: Context<'info, SettleLimitOrder<'info>>,
    hook_output: &[AccountInfo<'info>],
) -> Result<()> {
    let tick_spacing = ctx.accounts.pool_state.load()?.tick_spacing;

    let tick_index = ctx.accounts.limit_order.tick_index;
    let tick_array = ctx.accounts.tick_array.load()?;
    let tick_state = tick_array.get_tick_state(tick_index, tick_spacing)?;

    let order = &mut ctx.accounts.limit_order;
    let amount_out = order.settle_filled_order(tick_state)?;
    if order.get_unfilled_amount()? != 0 {
        require_gt!(amount_out, 0);
    }
    emit!(SettleLimitOrderEvent {
        pool_id: ctx.accounts.pool_state.key(),
        limit_order: ctx.accounts.limit_order.key(),
        zero_for_one: ctx.accounts.limit_order.zero_for_one,
        tick_index: ctx.accounts.limit_order.tick_index,
        total_amount: ctx.accounts.limit_order.total_amount,
        filled_amount: ctx.accounts.limit_order.filled_amount,
        settled_amount_out: amount_out,
    });
    // For monitoring purposes, perform the transfer even if amount_out is 0
    crate::util::transfer_checked_with_hook(
        &ctx.accounts.output_token_program.to_account_info(),
        &ctx.accounts.output_vault.to_account_info(),
        &ctx.accounts.output_vault_mint.to_account_info(),
        &ctx.accounts.output_token_account.to_account_info(),
        &ctx.accounts.pool_state.to_account_info(),
        hook_output,
        amount_out,
        ctx.accounts.output_vault_mint.decimals,
        &[&ctx.accounts.pool_state.load()?.seeds()],
    )?;

    Ok(())
}
