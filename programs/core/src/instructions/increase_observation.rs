use crate::states::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::system_instruction::create_account;
use std::mem::size_of;
use std::ops::DerefMut;

#[derive(Accounts)]
pub struct IncreaseObservation<'info> {
    /// Pays to increase storage slots for oracle observations
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Increase observation slots for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// To create new program accounts
    pub system_program: Program<'info, System>,
}

pub fn increase_observation_cardinality_next<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, IncreaseObservation<'info>>,
    observation_account_bumps: Vec<u8>,
) -> Result<()> {
    let pool_state = ctx.accounts.pool_state.deref_mut();
    let pool_key = pool_state.key();
    let mut i: usize = 0;
    while i < observation_account_bumps.len() {
        let observation_account_seeds = [
            &OBSERVATION_SEED.as_bytes(),
            pool_key.as_ref(),
            &(pool_state.observation_cardinality_next + i as u16).to_be_bytes(),
            &[observation_account_bumps[i]],
        ];

        require_keys_eq!(
            ctx.remaining_accounts[i].key(),
            Pubkey::create_program_address(&observation_account_seeds[..], &ctx.program_id)
                .unwrap()
        );
        msg!("ctx.remaining_accounts[i].key():{}, Pubkey::create_program_address(&observation_account_seeds[..], &ctx.program_id)
        .unwrap():{} ",ctx.remaining_accounts[i].key(), Pubkey::create_program_address(&observation_account_seeds[..], &ctx.program_id)
        .unwrap());
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

        let mut observation_state = Account::<ObservationState>::try_from_unchecked(
            &ctx.remaining_accounts[i].to_account_info(),
        )?;

        // this data will not be used because the initialized boolean is still false
        observation_state.bump = observation_account_bumps[i];
        observation_state.index = pool_state.observation_cardinality_next + i as u16;
        observation_state.block_timestamp = 1;

        // drop(observation_state);
        observation_state.exit(ctx.program_id)?;

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

    Ok(())
}
