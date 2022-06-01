use super::{swap, SwapContext};
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct SwapBaseInSingle<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The user token account for input token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub input_token_account: Account<'info, TokenAccount>,

    /// The user token account for output token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub output_token_account: Account<'info, TokenAccount>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Account<'info, TokenAccount>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Account<'info, TokenAccount>,

    /// The program account for the most recent oracle observation
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,
}

pub fn swap_base_in_single<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapBaseInSingle<'info>>,
    amount_in: u64,
    amount_out_minimum: u64,
    sqrt_price_limit_x32: u64,
) -> Result<()> {
    let amount_out = exact_input_internal(
        &mut SwapContext {
            signer: ctx.accounts.signer.clone(),
            amm_config: ctx.accounts.amm_config.as_mut(),
            input_token_account: ctx.accounts.input_token_account.clone(),
            output_token_account: ctx.accounts.output_token_account.clone(),
            input_vault: ctx.accounts.input_vault.clone(),
            output_vault: ctx.accounts.output_vault.clone(),
            token_program: ctx.accounts.token_program.clone(),
            pool_state: ctx.accounts.pool_state.as_mut(),
            last_observation_state: &mut ctx.accounts.last_observation_state,
        },
        ctx.remaining_accounts,
        amount_in,
        sqrt_price_limit_x32,
    )?;
    msg!("exact_input_single, amount_out: {}", amount_out);
    require!(
        amount_out >= amount_out_minimum,
        ErrorCode::TooLittleReceived
    );
    Ok(())
}

/// Performs a single exact input swap
pub fn exact_input_internal<'b, 'info>(
    accounts: &mut SwapContext<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_in: u64,
    sqrt_price_limit_x32: u64,
) -> Result<u64> {
    let zero_for_one = accounts.input_vault.mint == accounts.pool_state.token_mint_0;

    let balance_before = accounts.input_vault.amount;
    swap(
        accounts,
        remaining_accounts,
        i64::try_from(amount_in).unwrap(),
        if sqrt_price_limit_x32 == 0 {
            if zero_for_one {
                tick_math::MIN_SQRT_RATIO + 1
            } else {
                tick_math::MAX_SQRT_RATIO - 1
            }
        } else {
            sqrt_price_limit_x32
        },
    )?;

    accounts.input_vault.reload()?;
    Ok(accounts.input_vault.amount - balance_before)
}
