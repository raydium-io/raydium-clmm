use super::{exact_internal, SwapAccounts};
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
) -> Result<()> {
    let mut amount_in_internal = amount_in;
    let mut input_token_account = Box::new(ctx.accounts.input_token_account.clone());
    let mut accounts: &[AccountInfo] = ctx.remaining_accounts;
    while !accounts.is_empty() {
        let mut remaining_accounts = accounts.iter();
        let account_info = remaining_accounts.next().unwrap();
        if accounts.len() != ctx.remaining_accounts.len()
            && account_info.data_len() != AmmConfig::LEN
        {
            accounts = remaining_accounts.as_slice();
            continue;
        }
        let amm_config = Box::new(Account::<AmmConfig>::try_from(account_info)?);
        let mut pool_state_loader =
            AccountLoader::<PoolState>::try_from(remaining_accounts.next().unwrap())?;
        let output_token_account = Box::new(Account::<TokenAccount>::try_from(
            &remaining_accounts.next().unwrap(),
        )?);
        let input_vault = Box::new(Account::<TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let output_vault = Box::new(Account::<TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?);
        let mut observation_state =
            AccountLoader::<ObservationState>::try_from(remaining_accounts.next().unwrap())?;

        {
            let pool_state = pool_state_loader.load()?;
            // check observation account is owned by the pool
            require_keys_eq!(pool_state.observation_key, observation_state.key());
            // check ammConfig account is associate with the pool
            require_keys_eq!(pool_state.amm_config, amm_config.key());
        }

        let mut tick_array =
            AccountLoader::<TickArrayState>::try_from(remaining_accounts.next().unwrap())?;
        // solana_program::log::sol_log_compute_units();
        accounts = remaining_accounts.as_slice();
        amount_in_internal = exact_internal(
            &mut SwapAccounts {
                signer: ctx.accounts.payer.clone(),
                amm_config: &amm_config,
                input_token_account: input_token_account.clone(),
                pool_state: &mut pool_state_loader,
                output_token_account: output_token_account.clone(),
                input_vault: input_vault.clone(),
                output_vault: output_vault.clone(),
                tick_array_state: &mut tick_array,
                observation_state: &mut observation_state,
                token_program: ctx.accounts.token_program.clone(),
            },
            accounts,
            amount_in_internal,
            0,
            true,
        )?;
        // output token is the new swap input token
        input_token_account = output_token_account;
    }
    require!(
        amount_in_internal >= amount_out_minimum,
        ErrorCode::TooLittleOutputReceived
    );

    Ok(())
}
