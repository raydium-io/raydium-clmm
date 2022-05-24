use super::{swap, SwapContext};
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::collections::BTreeMap;

#[derive(Accounts)]
pub struct ExactInputSingle<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub factory_state: Box<Account<'info, FactoryState>>,

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

pub fn exact_input_single<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ExactInputSingle<'info>>,
    deadline: i64,
    amount_in: u64,
    amount_out_minimum: u64,
    sqrt_price_limit_x32: u64,
) -> Result<()> {
    let amount_out = exact_input_internal(
        &mut SwapContext {
            signer: ctx.accounts.signer.clone(),
            factory_state: ctx.accounts.factory_state.clone(),
            input_token_account: ctx.accounts.input_token_account.clone(),
            output_token_account: ctx.accounts.output_token_account.clone(),
            input_vault: ctx.accounts.input_vault.clone(),
            output_vault: ctx.accounts.output_vault.clone(),
            token_program: ctx.accounts.token_program.clone(),
            pool_state: ctx.accounts.pool_state.clone(),
            last_observation_state: ctx.accounts.last_observation_state.clone(),
        },
        ctx.remaining_accounts,
        amount_in,
        sqrt_price_limit_x32,
    )?;
    require!(
        amount_out >= amount_out_minimum,
        ErrorCode::TooLittleReceived
    );
    Ok(())
}

/// Performs a single exact input swap
pub fn exact_input_internal<'info>(
    accounts: &mut SwapContext<'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_in: u64,
    sqrt_price_limit_x32: u64,
) -> Result<u64> {
    let zero_for_one = accounts.input_vault.mint == accounts.pool_state.token_0;

    let balance_before = accounts.input_vault.amount;
    swap(
        Context::new(
            &crate::ID,
            accounts,
            remaining_accounts,
            BTreeMap::default(),
        ),
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
