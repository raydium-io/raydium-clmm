use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::system_instruction::create_account;
use std::mem::size_of;

#[derive(Accounts)]
pub struct IncreaseObservationCardinalityNextCtx<'info> {
    /// Pays to increase storage slots for oracle observations
    pub payer: Signer<'info>,

    /// Increase observation slots for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// To create new program accounts
    pub system_program: Program<'info, System>,
}

pub fn increase_observation_cardinality_next<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info,IncreaseObservationCardinalityNextCtx<'info>>,
    observation_account_bumps: Vec<u8>,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    require!(pool_state.unlocked, ErrorCode::LOK);
    pool_state.unlocked = false;
    
    let mut i: usize = 0;
    while i < observation_account_bumps.len() {
        let observation_account_seeds = [
            &OBSERVATION_SEED.as_bytes(),
            pool_state.token_0.as_ref(),
            pool_state.token_1.as_ref(),
            &pool_state.fee.to_be_bytes(),
            &(pool_state.observation_cardinality_next + i as u16).to_be_bytes(),
            &[observation_account_bumps[i]],
        ];

        require!(
            ctx.remaining_accounts[i].key()
                == Pubkey::create_program_address(&observation_account_seeds[..], &ctx.program_id)
                    .unwrap(),
            ErrorCode::OS
        );

        let space = 8 + size_of::<ObservationState>();
        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(space);
        let ix = create_account(
            ctx.accounts.payer.key,
            &ctx.remaining_accounts[i].key,
            lamports,
            space as u64,
            ctx.program_id,
        );

        solana_program::program::invoke_signed(
            &ix,
            &[
                ctx.accounts.payer.to_account_info(),
                ctx.remaining_accounts[i].to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&observation_account_seeds[..]],
        )?;

        let observation_state_loader = AccountLoader::<ObservationState>::try_from_unchecked(
            &crate::id(),
            &ctx.remaining_accounts[i].to_account_info(),
        )?;
        let mut observation_state = observation_state_loader.load_init()?;
        // this data will not be used because the initialized boolean is still false
        observation_state.bump = observation_account_bumps[i];
        observation_state.index = pool_state.observation_cardinality_next + i as u16;
        observation_state.block_timestamp = 1;

        drop(observation_state);
        observation_state_loader.exit(ctx.program_id)?;

        i += 1;
    }
    let observation_cardinality_next_old = pool_state.observation_cardinality_next;
    pool_state.observation_cardinality_next = pool_state
        .observation_cardinality_next
        .checked_add(i as u16)
        .unwrap();

    emit!(oracle::IncreaseObservationCardinalityNext {
        observation_cardinality_next_old,
        observation_cardinality_next_new: pool_state.observation_cardinality_next,
    });

    pool_state.unlocked = true;
    Ok(())
}
