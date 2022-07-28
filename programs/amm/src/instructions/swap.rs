use super::{swap_internal, SwapContext};
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::Neg;

#[derive(Accounts)]
pub struct SwapSingle<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Account<'info, TokenAccount>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Account<'info, TokenAccount>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Account<'info, TokenAccount>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Account<'info, TokenAccount>,

    #[account(mut, constraint = tick_array.load()?.amm_pool == pool_state.key())]
    pub tick_array: AccountLoader<'info, TickArrayState>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,
}

pub fn swap<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let amount = exact_internal(
        &mut SwapContext {
            signer: ctx.accounts.payer.clone(),
            amm_config: ctx.accounts.amm_config.as_mut(),
            input_token_account: ctx.accounts.input_token_account.clone(),
            output_token_account: ctx.accounts.output_token_account.clone(),
            input_vault: ctx.accounts.input_vault.clone(),
            output_vault: ctx.accounts.output_vault.clone(),
            token_program: ctx.accounts.token_program.clone(),
            pool_state: ctx.accounts.pool_state.as_mut(),
            tick_array_state: &mut ctx.accounts.tick_array,
            observation_state: &mut ctx.accounts.observation_state,
        },
        ctx.remaining_accounts,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
    )?;
    if is_base_input {
        require!(
            amount >= other_amount_threshold,
            ErrorCode::TooLittleOutputReceived
        );
    } else {
        require!(
            amount <= other_amount_threshold,
            ErrorCode::TooMuchInputPaid
        );
    }

    Ok(())
}

/// Performs a single exact input/output swap
/// if is_base_input = true, return vaule is the max_amount_out, otherwise is min_amount_in
pub fn exact_internal<'b, 'info>(
    ctx: &mut SwapContext<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<u64> {
    let pool_state_info = ctx.pool_state.to_account_info();
    let zero_for_one = ctx.input_vault.mint == ctx.pool_state.token_mint_0;
    let input_balance_before = ctx.input_vault.amount;
    let output_balance_before = ctx.output_vault.amount;

    let mut amount_specified = i64::try_from(amount_specified).unwrap();
    if !is_base_input {
        amount_specified = -i64::try_from(amount_specified).unwrap();
    };

    let (amount_0, amount_1) = swap_internal(
        ctx,
        remaining_accounts,
        amount_specified,
        if sqrt_price_limit_x64 == 0 {
            if zero_for_one {
                tick_math::MIN_SQRT_RATIO_X64 + 1
            } else {
                tick_math::MAX_SQRT_RATIO_X64 - 1
            }
        } else {
            sqrt_price_limit_x64
        },
        zero_for_one,
    )?;

    #[cfg(feature = "enable-log")]
    msg!(
        "exact_swap_internal, is_base_input:{}, amount_0: {}, amount_1: {}",
        is_base_input,
        amount_0,
        amount_1
    );
    require!(amount_0 != 0 && amount_1 != 0, ErrorCode::TooSmallInputOrOutputAmount);

    let (token_account_0, token_account_1, vault_0, vault_1) = if zero_for_one {
        (
            ctx.input_token_account.clone(),
            ctx.output_token_account.clone(),
            ctx.input_vault.clone(),
            ctx.output_vault.clone(),
        )
    } else {
        (
            ctx.output_token_account.clone(),
            ctx.input_token_account.clone(),
            ctx.output_vault.clone(),
            ctx.input_vault.clone(),
        )
    };
    assert!(vault_0.key() == ctx.pool_state.token_vault_0);
    assert!(vault_1.key() == ctx.pool_state.token_vault_1);

    if zero_for_one {
        //  x -> y, deposit x token from user to pool vault.
        if amount_0 > 0 {
            transfer_from_user_to_pool_vault(
                &ctx.signer,
                &token_account_0,
                &vault_0,
                &ctx.token_program,
                amount_0 as u64,
            )?;
            ctx.pool_state.swap_in_amount_token_0 += amount_0 as u128;
        }
        // x -> y，transfer y token from pool vault to user.
        if amount_1 < 0 {
            transfer_from_pool_vault_to_user(
                ctx.pool_state,
                &vault_1,
                &token_account_1,
                &ctx.token_program,
                amount_1.neg() as u64,
            )?;
            ctx.pool_state.swap_out_amount_token_1 += amount_1.neg() as u128;
        }
    } else {
        if amount_1 > 0 {
            transfer_from_user_to_pool_vault(
                &ctx.signer,
                &token_account_1,
                &vault_1,
                &ctx.token_program,
                amount_1 as u64,
            )?;
            ctx.pool_state.swap_in_amount_token_1 += amount_1 as u128;
        }
        if amount_0 < 0 {
            transfer_from_pool_vault_to_user(
                ctx.pool_state,
                &vault_0,
                &token_account_0,
                &ctx.token_program,
                amount_0.neg() as u64,
            )?;
            ctx.pool_state.swap_out_amount_token_0 += amount_0.neg() as u128;
        }
    }

    emit!(SwapEvent {
        pool_state: pool_state_info.key(),
        sender: ctx.signer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0,
        amount_1,
        sqrt_price_x64: ctx.pool_state.sqrt_price_x64,
        liquidity: ctx.pool_state.liquidity,
        tick: ctx.pool_state.tick_current
    });

    ctx.input_vault.reload()?;
    ctx.output_vault.reload()?;

    if is_base_input {
        Ok(output_balance_before
            .checked_sub(ctx.output_vault.amount)
            .unwrap())
    } else {
        Ok(ctx
            .input_vault
            .amount
            .checked_sub(input_balance_before)
            .unwrap())
    }
}
