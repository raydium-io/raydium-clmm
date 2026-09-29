use crate::error::ErrorCode;
use crate::states::*;
use crate::util::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::Token2022;
#[derive(Accounts)]
pub struct CollectExcessLamports<'info> {
    /// Only admin or collect_lamports can collect lamports
    #[account(
        mut,
        constraint = (collect_lamports_wallet.key() == crate::collect_lamports::ID || collect_lamports_wallet.key() == crate::admin::ID) @ ErrorCode::NotApproved
    )]
    pub collect_lamports_wallet: Signer<'info>,

    /// Pool state stores accumulated protocol fee amount
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,

    /// The SPL program 2022 to perform token transfers
    pub token_program_2022: Program<'info, Token2022>,
    // remaining account
    // It can be vaults, LP mints, or PDA accounts.
    // `..+M` `[writable]` M source lamports accounts.
}

pub fn collect_excess_lamports<'info>(
    ctx: Context<'info, CollectExcessLamports<'info>>,
) -> Result<()> {
    // The token sources are drained through a CPI, the program owned sources by
    // moving lamports directly. Those two cannot be interleaved: the runtime only
    // pushes the caller's lamports changes into its own accounts for the accounts a
    // CPI actually carries, so a pda debited before a CPI leaves the
    // transaction wide lamports delta non-zero and the next CPI aborts with
    // `UnbalancedInstruction` ("sum of account balances before and after
    // instruction do not match"). `remaining_accounts` is ordered by the caller, so
    // run every CPI first and only then touch lamports directly.
    for source_lamports_account in ctx.remaining_accounts.iter() {
        let token_program = if *source_lamports_account.owner == Token::id() {
            ctx.accounts.token_program.to_account_info()
        } else if *source_lamports_account.owner == Token2022::id() {
            ctx.accounts.token_program_2022.to_account_info()
        } else {
            continue;
        };
        withdraw_excess_lamports_from_token(
            token_program,
            source_lamports_account.to_account_info(),
            ctx.accounts.collect_lamports_wallet.to_account_info(),
            ctx.accounts.pool_state.to_account_info(),
            &[&ctx.accounts.pool_state.load()?.seeds()],
        )?;
    }

    // process pool_state
    withdraw_excess_lamports_from_pda(
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.collect_lamports_wallet.to_account_info(),
    )?;

    for source_lamports_account in ctx.remaining_accounts.iter() {
        if *source_lamports_account.owner == crate::id() {
            withdraw_excess_lamports_from_pda(
                source_lamports_account,
                &ctx.accounts.collect_lamports_wallet.to_account_info(),
            )?;
        }
    }
    Ok(())
}

fn withdraw_excess_lamports_from_pda(
    source_lamports_account: &AccountInfo,
    collect_lamports_wallet: &AccountInfo,
) -> Result<()> {
    let rent = Rent::get()?;
    let minimum_balance = rent.minimum_balance(source_lamports_account.data_len());
    let source_lamports = source_lamports_account.lamports();
    let excess_lamports = source_lamports
        .checked_sub(minimum_balance)
        .ok_or(ProgramError::InsufficientFunds)?;

    if excess_lamports == 0 {
        return Ok(());
    }

    {
        let mut source_lamports_ref = source_lamports_account.try_borrow_mut_lamports()?;

        **source_lamports_ref = source_lamports
            .checked_sub(excess_lamports)
            .ok_or(ProgramError::InsufficientFunds)?;
    }

    {
        let mut destination_lamports_ref = collect_lamports_wallet.try_borrow_mut_lamports()?;

        **destination_lamports_ref = destination_lamports_ref
            .checked_add(excess_lamports)
            .ok_or(ProgramError::ArithmeticOverflow)?;
    }
    return Ok(());
}
