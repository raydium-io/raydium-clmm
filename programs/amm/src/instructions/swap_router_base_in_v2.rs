use crate::error::ErrorCode;
use crate::instructions::swap_router_base_in::SwapRouterBaseIn;
use crate::instructions::swap_v2::{exact_internal_v3, SwapSingleV2};
use crate::states::*;
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

/// Accounts each hop reads positionally, before its own tape:
/// amm_config, pool_state, output_token_account, input_vault, output_vault,
/// output_token_mint, observation_state.
const HOP_NAMED_ACCOUNTS: usize = 7;

/// `swap_router_base_in` with one account tape per hop.
///
/// The old version found hop boundaries by looking for an account of `AmmConfig::LEN`,
/// which leaves no room to append hook accounts — a hook account of that exact size would
/// be read as the start of another hop. Hop length is now stated by the caller.
pub fn swap_router_base_in_v2<'info>(
    ctx: Context<'info, SwapRouterBaseIn<'info>>,
    amount_in: u64,
    amount_out_minimum: u64,
    remaining_accounts_info: Vec<RemainingAccountsInfo>,
) -> Result<()> {
    let mut amount_in_internal = amount_in;
    let mut input_token_account = Box::new(ctx.accounts.input_token_account.clone());
    let mut input_token_mint = Box::new(ctx.accounts.input_token_mint.clone());
    let mut accounts: &[AccountInfo<'info>] = ctx.remaining_accounts;

    require!(
        !remaining_accounts_info.is_empty(),
        ErrorCode::RemainingAccountsInfoMismatch
    );

    for (hop, hop_info) in remaining_accounts_info.iter().enumerate() {
        // The previous hop reloaded only its own copy of this account.
        if hop > 0 {
            input_token_account.reload()?;
        }
        require_gte!(accounts.len(), HOP_NAMED_ACCOUNTS, ErrorCode::AccountLack);
        let (named, rest) = accounts.split_at(HOP_NAMED_ACCOUNTS);

        let tape_len: usize = hop_info
            .slices
            .iter()
            .try_fold(0usize, |acc, slice| acc.checked_add(slice.length as usize))
            .ok_or(ErrorCode::RemainingAccountsInfoMismatch)?;
        require_gte!(
            rest.len(),
            tape_len,
            ErrorCode::RemainingAccountsInfoMismatch
        );
        let (tape, next) = rest.split_at(tape_len);
        let parsed = parse_remaining_accounts(
            tape,
            hop_info,
            &[
                AccountsType::TickArray,
                AccountsType::TickArrayBitmapExtension,
                AccountsType::TransferHookInput,
                AccountsType::TransferHookOutput,
            ],
        )?;

        let amm_config = Box::new(Account::<AmmConfig>::try_from(&named[0])?);
        let pool_state_loader = AccountLoader::<PoolState>::try_from(&named[1])?;
        let output_token_account = Box::new(InterfaceAccount::<TokenAccount>::try_from(&named[2])?);
        let input_vault = Box::new(InterfaceAccount::<TokenAccount>::try_from(&named[3])?);
        let output_vault = Box::new(InterfaceAccount::<TokenAccount>::try_from(&named[4])?);
        let output_token_mint = Box::new(InterfaceAccount::<Mint>::try_from(&named[5])?);
        let observation_state = AccountLoader::<ObservationState>::try_from(&named[6])?;

        {
            let pool_state = pool_state_loader.load()?;
            require_keys_eq!(pool_state.observation_key, observation_state.key());
            require_keys_eq!(pool_state.amm_config, amm_config.key());
        }
        // Checked here so a wrong mint fails with a clear error instead of inside the CPI.
        require_keys_eq!(output_token_mint.key(), output_vault.mint);

        let tick_arrays: Vec<&AccountInfo> = parsed.tick_array.iter().collect();
        amount_in_internal = exact_internal_v3(
            &mut SwapSingleV2 {
                payer: ctx.accounts.payer.clone(),
                amm_config,
                input_token_account: input_token_account.clone(),
                pool_state: pool_state_loader,
                output_token_account: output_token_account.clone(),
                input_vault: input_vault.clone(),
                output_vault: output_vault.clone(),
                input_vault_mint: input_token_mint.clone(),
                output_vault_mint: output_token_mint.clone(),
                observation_state,
                token_program: ctx.accounts.token_program.clone(),
                token_program_2022: ctx.accounts.token_program_2022.clone(),
                memo_program: ctx.accounts.memo_program.clone(),
            },
            tick_arrays.as_slice(),
            parsed.bitmap_extension(),
            parsed.hook_input,
            parsed.hook_output,
            amount_in_internal,
            0,
            true,
        )?;
        // output token is the new swap input token
        input_token_account = output_token_account;
        input_token_mint = output_token_mint;
        accounts = next;
    }

    // One tape per hop, and nothing left over: a trailing account means the caller and the
    // program disagree about how many hops there are.
    require!(
        accounts.is_empty(),
        ErrorCode::RemainingAccountsInfoMismatch
    );
    require_gte!(
        amount_in_internal,
        amount_out_minimum,
        ErrorCode::TooLittleOutputReceived
    );

    Ok(())
}
