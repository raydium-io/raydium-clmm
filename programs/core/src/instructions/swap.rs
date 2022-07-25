use super::{swap_internal, SwapContext};
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

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

    /// The program account for the most recent oracle observation
    #[account(mut)]
    pub last_observation: Box<Account<'info, ObservationState>>,

    #[account(mut)]
    pub next_observation: Box<Account<'info, ObservationState>>,

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
            last_observation_state: &mut ctx.accounts.last_observation,
            next_observation_state: &mut ctx.accounts.next_observation,
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
    accounts: &mut SwapContext<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<u64> {
    let zero_for_one = accounts.input_vault.mint == accounts.pool_state.token_mint_0;
    let input_balance_before = accounts.input_vault.amount;
    let output_balance_before = accounts.output_vault.amount;

    let mut amount_specified = i64::try_from(amount_specified).unwrap();
    if !is_base_input {
        amount_specified = -i64::try_from(amount_specified).unwrap();
    };

    swap_internal(
        accounts,
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

    accounts.input_vault.reload()?;
    accounts.output_vault.reload()?;
    #[cfg(feature = "enable-log")]
    msg!(
        "exact_swap_internal, is_base_input:{}, amount_in: {}, amount_out: {}",
        is_base_input,
        accounts.input_vault.amount - input_balance_before,
        output_balance_before - accounts.output_vault.amount
    );
    if is_base_input {
        Ok(output_balance_before
            .checked_sub(accounts.output_vault.amount)
            .unwrap())
    } else {
        Ok(accounts
            .input_vault
            .amount
            .checked_sub(input_balance_before)
            .unwrap())
    }
}
