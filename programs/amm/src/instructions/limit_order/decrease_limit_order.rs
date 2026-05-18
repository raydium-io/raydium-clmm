use crate::states::*;
use crate::util::{get_transfer_fee, transfer_from_pool_vault_to_user};
use crate::{error::ErrorCode, Result};
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};
#[derive(Accounts)]
pub struct DecreaseLimitOrder<'info> {
    pub owner: Signer<'info>,

    #[account(mut)]
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

    /// The owner's output token account (opposite direction from input)
    #[account(
        mut,
        token::mint = output_vault.mint,
        token::authority = owner,
    )]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

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

    /// The mint of token vault
    #[account(
        address = input_vault.mint
    )]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output vault
    #[account(
        address = output_vault.mint
    )]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// SPL program 2022 for token transfers
    pub token_program_2022: Program<'info, Token2022>,
    // remaining account, for tick array bitmap extension (optional)
    // #[account(
    //     seeds = [
    //         POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
    //         pool_state.key().as_ref(),
    //     ],
    //     bump
    // )]
    // pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,
}

pub fn decrease_limit_order<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLimitOrder<'info>>,
    amount: u64,
    amount_min: u64,
) -> Result<()> {
    require!(amount > 0, ErrorCode::ZeroAmountSpecified);

    let tick_spacing = ctx.accounts.pool_state.load()?.tick_spacing;
    let order = &mut ctx.accounts.limit_order;

    let mut tick_array = ctx.accounts.tick_array.load_mut()?;
    let tick_state = tick_array.get_tick_state_mut(order.tick_index, tick_spacing)?;
    let tick_initialized_before = tick_state.is_initialized();

    // In-place decrease: settle first, then subtract
    let DecreaseAmountResult {
        settled_output_amount,
        real_decrease_amount,
    } = order.decrease_amount(tick_state, amount)?;

    emit!(DecreaseLimitOrderEvent {
        pool_id: ctx.accounts.pool_state.key(),
        limit_order: order.key(),
        zero_for_one: order.zero_for_one,
        tick_index: order.tick_index,
        total_amount: order.total_amount,
        filled_amount: order.filled_amount,
        settled_output_amount,
        decreased_amount: real_decrease_amount,
    });

    // Handle tick deinitialization
    if tick_initialized_before && !tick_state.is_initialized() {
        tick_array.update_initialized_tick_count(false)?;

        if tick_array.initialized_tick_count == 0 {
            let mut pool_state = ctx.accounts.pool_state.load_mut()?;

            let tick_array_start_index =
                TickArrayState::get_array_start_index(order.tick_index, tick_spacing);

            let use_tickarray_bitmap_extension =
                pool_state.is_overflow_default_tickarray_bitmap(vec![tick_array_start_index]);

            let tickarray_bitmap_extension = if use_tickarray_bitmap_extension {
                require!(ctx.remaining_accounts.len() > 0, ErrorCode::AccountLack);
                Some(&ctx.remaining_accounts[0])
            } else {
                None
            };

            pool_state.flip_tick_array_bit(tickarray_bitmap_extension, tick_array_start_index)?;
        }
    }

    if settled_output_amount > 0 {
        transfer_from_pool_vault_to_user(
            &ctx.accounts.pool_state,
            &ctx.accounts.output_vault.to_account_info(),
            &ctx.accounts.output_token_account.to_account_info(),
            Some(ctx.accounts.output_vault_mint.clone()),
            &ctx.accounts.token_program,
            Some(ctx.accounts.token_program_2022.to_account_info()),
            settled_output_amount,
        )?;
    }

    if real_decrease_amount > 0 {
        let transfer_fee =
            get_transfer_fee(ctx.accounts.input_vault_mint.clone(), real_decrease_amount)?;
        let amount_without_transfer_fee = real_decrease_amount
            .checked_sub(transfer_fee)
            .ok_or(ErrorCode::CalculateOverflow)?;
        require_gte!(
            amount_without_transfer_fee,
            amount_min,
            ErrorCode::PriceSlippageCheck
        );
        transfer_from_pool_vault_to_user(
            &ctx.accounts.pool_state,
            &ctx.accounts.input_vault.to_account_info(),
            &ctx.accounts.input_token_account.to_account_info(),
            Some(ctx.accounts.input_vault_mint.clone()),
            &ctx.accounts.token_program,
            Some(ctx.accounts.token_program_2022.to_account_info()),
            real_decrease_amount,
        )?;
    }

    Ok(())
}
