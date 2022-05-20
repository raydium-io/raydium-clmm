use super::{exact_input_internal, SwapContext};
use crate::error::ErrorCode;
use crate::program::AmmCore;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct ExactInput<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub factory_state: UncheckedAccount<'info>,

    /// The token account that pays input tokens for the swap
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub input_token_account: UncheckedAccount<'info>,

    /// The core program where swap is performed
    pub core_program: Program<'info, AmmCore>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,
}

pub fn exact_input<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ExactInput<'info>>,
    deadline: i64,
    amount_in: u64,
    amount_out_minimum: u64,
    additional_accounts_per_pool: Vec<u8>,
) -> Result<()> {
    let mut remaining_accounts = ctx.remaining_accounts.iter();

    let mut amount_in_internal = amount_in;
    let mut input_token_account = ctx.accounts.input_token_account.clone();
    for i in 0..additional_accounts_per_pool.len() {
        let pool_state = UncheckedAccount::try_from(remaining_accounts.next().unwrap().clone());
        let output_token_account =
            UncheckedAccount::try_from(remaining_accounts.next().unwrap().clone());
        let input_vault = Box::new(Account::<TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let output_vault = Box::new(Account::<TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?);

        amount_in_internal = exact_input_internal(
            &mut SwapContext {
                signer: ctx.accounts.signer.clone(),
                factory_state: ctx.accounts.factory_state.clone(),
                input_token_account: input_token_account.clone(),
                pool_state,
                output_token_account: output_token_account.clone(),
                input_vault,
                output_vault,
                last_observation_state: UncheckedAccount::try_from(
                    remaining_accounts.next().unwrap().clone(),
                ),
                token_program: ctx.accounts.token_program.clone(),
                callback_handler: UncheckedAccount::try_from(
                    ctx.accounts.core_program.to_account_info(),
                ),
            },
            remaining_accounts.as_slice(),
            amount_in_internal,
            0,
        )?;

        if i < additional_accounts_per_pool.len() - 1 {
            // reach accounts needed for the next swap
            for _j in 0..additional_accounts_per_pool[i] {
                remaining_accounts.next();
            }
            // output token account is the new input
            input_token_account = output_token_account;
        }
    }
    require!(
        amount_in_internal >= amount_out_minimum,
        ErrorCode::TooLittleReceived
    );

    Ok(())
}
