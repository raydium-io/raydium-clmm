use super::{exact_input_internal, SwapContext};
use crate::error::ErrorCode;
use crate::program::AmmCore;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct ExactInputSingle<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub factory_state: UncheckedAccount<'info>,

    /// The program account of the pool in which the swap will be performed
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// The user token account for input token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub input_token_account: UncheckedAccount<'info>,

    /// The user token account for output token
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub output_token_account: UncheckedAccount<'info>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Box<Account<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Box<Account<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: UncheckedAccount<'info>,

    /// The core program where swap is performed
    pub core_program: Program<'info, AmmCore>,

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
            // next_observation_state: ctx.accounts.next_observation_state.clone(),
            callback_handler: UncheckedAccount::try_from(
                ctx.accounts.core_program.to_account_info(),
            ),
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
