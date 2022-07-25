use super::{exact_internal, SwapContext};
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct SwapRouterBaseIn<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The token account that pays input tokens for the swap
    #[account(mut)]
    pub input_token_account: Account<'info, TokenAccount>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,
}

pub fn swap_router_base_in<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapRouterBaseIn<'info>>,
    amount_in: u64,
    amount_out_minimum: u64,
    additional_accounts_per_pool: Vec<u8>,
) -> Result<()> {
    let mut remaining_accounts = ctx.remaining_accounts.iter();
    let mut amount_in_internal = amount_in;
    let mut input_token_account = ctx.accounts.input_token_account.clone();

    for i in 0..additional_accounts_per_pool.len() {
        let mut amm_config = Box::new(Account::<AmmConfig>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let mut pool_state = Box::new(Account::<PoolState>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let mut output_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let input_vault = Account::<TokenAccount>::try_from(remaining_accounts.next().unwrap())?;
        let output_vault = Account::<TokenAccount>::try_from(remaining_accounts.next().unwrap())?;
        let mut tick_array =  AccountLoader::<TickArrayState>::try_from(remaining_accounts.next().unwrap())?;
        let mut observation_state = AccountLoader::<ObservationState>::try_from(remaining_accounts.next().unwrap())?;
        require_keys_eq!(pool_state.observation_key, observation_state.key());
        solana_program::log::sol_log_compute_units();
        amount_in_internal = exact_internal(
            &mut SwapContext {
                signer: ctx.accounts.payer.clone(),
                amm_config: amm_config.as_mut(),
                input_token_account: input_token_account.clone(),
                pool_state: pool_state.as_mut(),
                output_token_account: output_token_account.clone(),
                input_vault: input_vault.clone(),
                output_vault: output_vault.clone(),
                tick_array_state: &mut tick_array,
                observation_state: &mut observation_state,
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
        observation_state.exit(&crate::id())?;

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
