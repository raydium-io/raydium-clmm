use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_2022;
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
}

pub fn settle_limit_order(ctx: Context<SettleLimitOrder>) -> Result<()> {
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
    token_2022::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.output_token_program.to_account_info(),
            token_2022::TransferChecked {
                from: ctx.accounts.output_vault.to_account_info(),
                to: ctx.accounts.output_token_account.to_account_info(),
                authority: ctx.accounts.pool_state.to_account_info(),
                mint: ctx.accounts.output_vault_mint.to_account_info(),
            },
            &[&ctx.accounts.pool_state.load()?.seeds()],
        ),
        amount_out,
        ctx.accounts.output_vault_mint.decimals,
    )?;

    Ok(())
}
