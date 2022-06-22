use super::{exact_internal, SwapContext};
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct SwapBaseIn<'info> {
    /// The user performing the swap
    pub signer: Signer<'info>,

    /// The factory state to read protocol fees
    /// CHECK: Safety check performed inside function body
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The token account that pays input tokens for the swap
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub input_token_account: Account<'info, TokenAccount>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,
}

pub fn swap_router_base_in<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapBaseIn<'info>>,
    amount_in: u64,
    amount_out_minimum: u64,
    additional_accounts_per_pool: Vec<u8>,
) -> Result<()> {
    let mut remaining_accounts = ctx.remaining_accounts.iter();
    let mut amount_in_internal = amount_in;
    let mut input_token_account = ctx.accounts.input_token_account.clone();

    for i in 0..additional_accounts_per_pool.len() {
        let mut pool_state = Box::new(Account::<PoolState>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let mut output_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let input_vault = Account::<TokenAccount>::try_from(remaining_accounts.next().unwrap())?;
        let output_vault = Account::<TokenAccount>::try_from(remaining_accounts.next().unwrap())?;
        let mut last_observation_state = Box::new(Account::<ObservationState>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        solana_program::log::sol_log_compute_units();
        amount_in_internal = exact_internal(
            &mut SwapContext {
                signer: ctx.accounts.signer.clone(),
                amm_config: ctx.accounts.amm_config.as_mut(),
                input_token_account: input_token_account.clone(),
                pool_state: pool_state.as_mut(),
                output_token_account: output_token_account.clone(),
                input_vault: input_vault.clone(),
                output_vault: output_vault.clone(),
                last_observation_state: &mut last_observation_state,
                token_program: ctx.accounts.token_program.clone(),
            },
            remaining_accounts.as_slice(),
            amount_in_internal,
            0,
            true,
        )?;
        // solana_program::log::sol_log_compute_units();
        output_token_account.reload()?;
        pool_state.exit(&crate::id())?;
        last_observation_state.exit(&crate::id())?;

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
        ErrorCode::TooLittleOutputReceived
    );

    Ok(())
}
