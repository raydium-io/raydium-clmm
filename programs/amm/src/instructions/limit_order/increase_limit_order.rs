use super::check_limit_order_amount;
use crate::states::*;
use crate::util::get_transfer_fee;
use crate::{error::ErrorCode, Result};
use anchor_lang::prelude::*;
use anchor_spl::token_2022;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct IncreaseLimitOrder<'info> {
    pub owner: Signer<'info>,

    #[account()]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        constraint = tick_array.load()?.pool_id == pool_state.key(),
    )]
    pub tick_array: AccountLoader<'info, TickArrayState>,

    #[account(
        mut,
        constraint = limit_order.pool_id == pool_state.key() && limit_order.owner == owner.key(),
    )]
    pub limit_order: Account<'info, LimitOrderState>,

    /// The payer's limit order token account
    #[account(
        mut,
        token::mint = input_vault.mint,
        token::authority = owner,
    )]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds limit order token
    #[account(
        mut,
        constraint = if limit_order.zero_for_one {
            input_vault.key() == pool_state.load()?.token_vault_0
        } else {
            input_vault.key() == pool_state.load()?.token_vault_1
        }
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint of token vault
    #[account(
        address = input_vault.mint
    )]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL-TOKEN / SPL-TOKEN2022 program for token transfers
    #[account(
        address = *input_vault_mint.to_account_info().owner
    )]
    pub input_token_program: Interface<'info, TokenInterface>,
}

pub fn increase_limit_order(ctx: Context<IncreaseLimitOrder>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::ZeroAmountSpecified);
    let tick_spacing = {
        let pool_state = ctx.accounts.pool_state.load()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::LimitOrder)
            || !pool_state.get_status_by_bit(PoolStatusBitIndex::Swap)
        {
            return err!(ErrorCode::NotApproved);
        }
        pool_state.tick_spacing
    };
    let tick_index = ctx.accounts.limit_order.tick_index;
    let mut tick_array = ctx.accounts.tick_array.load_mut()?;
    let tick_state = tick_array.get_tick_state_mut(tick_index, tick_spacing)?;

    let transfer_fee = get_transfer_fee(ctx.accounts.input_vault_mint.clone(), amount)?;
    let amount_without_transfer_fee = amount
        .checked_sub(transfer_fee)
        .ok_or(ErrorCode::CalculateOverflow)?;

    ctx.accounts
        .limit_order
        .increase_amount(tick_state, amount_without_transfer_fee)?;

    emit!(IncreaseLimitOrderEvent {
        pool_id: ctx.accounts.pool_state.key(),
        limit_order: ctx.accounts.limit_order.key(),
        zero_for_one: ctx.accounts.limit_order.zero_for_one,
        tick_index: ctx.accounts.limit_order.tick_index,
        total_amount: ctx.accounts.limit_order.total_amount,
        increased_amount: amount_without_transfer_fee,
        transfer_fee: transfer_fee,
    });

    check_limit_order_amount(
        ctx.accounts.limit_order.total_amount,
        tick_index,
        ctx.accounts.limit_order.zero_for_one,
    )?;

    token_2022::transfer_checked(
        CpiContext::new(
            ctx.accounts.input_token_program.to_account_info(),
            token_2022::TransferChecked {
                from: ctx.accounts.input_token_account.to_account_info(),
                to: ctx.accounts.input_vault.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
                mint: ctx.accounts.input_vault_mint.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.input_vault_mint.decimals,
    )?;

    Ok(())
}
