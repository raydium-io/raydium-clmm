use std::str::FromStr;
use crate::{ObservationState, PoolState};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{program::invoke, system_instruction};
use anchor_spl::{token::Token, token_2022::spl_token_2022};


#[derive(Accounts)]
pub struct RsizeObservation<'info> {
    /// The position owner or delegated authority
    #[account(mut,
        address = Pubkey::from_str("RayyNewmYYuLtrZvS1HKxYc5EXAcs6EqDpgoqrCETPj").unwrap()
    )]
    pub payer: Signer<'info>,
    /// pool state
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
    /// obseration account
    #[account(mut, 
        address = pool_state.load()?.observation_key,
        realloc = ObservationState::LEN,
        realloc::payer = payer,
        realloc::zero = false,
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,
    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn rsize_observation(ctx: Context<RsizeObservation>) -> Result<()> {
    let required_lamports = Rent::get()?.minimum_balance(ObservationState::LEN);

    let observation_account_info = ctx.accounts.observation_state.to_account_info();
    let original_lamports = observation_account_info.lamports();
    if original_lamports > required_lamports {
        let diff_lamports = original_lamports.checked_sub(required_lamports).unwrap();

        invoke(
            &system_instruction::transfer(
                observation_account_info.key,
                ctx.accounts.payer.key,
                diff_lamports,
            ),
            &[
                observation_account_info,
                ctx.accounts.payer.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        invoke(
            &spl_token_2022::instruction::sync_native(
                ctx.accounts.token_program.key,
                &ctx.accounts.payer.key(),
            )?,
            &[
                ctx.accounts.token_program.to_account_info(),
                ctx.accounts.payer.to_account_info(),
            ],
        )?;
    }

    ctx.accounts
        .observation_state
        .load_mut()?
        .initialize(ctx.accounts.pool_state.key())?;

    ctx.accounts.pool_state.load_mut()?.padding3 = 0;
    ctx.accounts.pool_state.load_mut()?.padding4 = 0;
    Ok(())
}
