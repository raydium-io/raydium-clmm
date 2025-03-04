use crate::states::*;
use anchor_lang::prelude::*;

use super::{calculate_latest_token_fees, update_position};
use crate::{PersonalPositionState, ProtocolPositionState};

#[derive(Accounts)]
pub struct UpdatePersonalRewards<'info> {
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &personal_position.tick_lower_index.to_be_bytes(),
            &personal_position.tick_upper_index.to_be_bytes(),
        ],
        bump,
        constraint = protocol_position.pool_id == pool_state.key(),
    )]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    /// CHECK: Account to store data for the position's lower tick
    #[account(mut)]
    pub tick_array_lower_loader: AccountLoader<'info, TickArrayState>,

    /// CHECK: Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_array_upper_loader: AccountLoader<'info, TickArrayState>,

    /// Increase liquidity for this position
    #[account(
        mut,
        constraint = personal_position.pool_id == pool_state.key()
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,
}

pub fn update_personal_rewards(ctx: Context<UpdatePersonalRewards>) -> Result<()> {
    let clock = Clock::get()?;
    let timestamp: u64 = u64::try_from(clock.unix_timestamp).unwrap();
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;

    let personal_position = &mut ctx.accounts.personal_position;
    let protocol_position = &mut ctx.accounts.protocol_position;

    let tick_array_lower_loader = &mut ctx.accounts.tick_array_lower_loader;
    let tick_array_upper_loader = &mut ctx.accounts.tick_array_upper_loader;

    validate_tick_state_address(
        &tick_array_lower_loader.key(),
        &pool_state,
        protocol_position.tick_lower_index,
    )?;

    validate_tick_state_address(
        &tick_array_upper_loader.key(),
        &pool_state,
        protocol_position.tick_upper_index,
    )?;

    let mut tick_lower_state = *tick_array_lower_loader
        .load_mut()?
        .get_tick_state_mut(protocol_position.tick_lower_index, pool_state.tick_spacing)?;
    let mut tick_upper_state = *tick_array_upper_loader
        .load_mut()?
        .get_tick_state_mut(protocol_position.tick_upper_index, pool_state.tick_spacing)?;

    update_position(
        0,
        &mut pool_state,
        protocol_position,
        &mut tick_lower_state,
        &mut tick_upper_state,
        timestamp,
    )?;

    update_personal_from_protocol(personal_position, protocol_position)?;

    tick_array_lower_loader.load_mut()?.update_tick_state(
        protocol_position.tick_lower_index,
        pool_state.tick_spacing,
        tick_lower_state,
    )?;

    tick_array_upper_loader.load_mut()?.update_tick_state(
        protocol_position.tick_upper_index,
        pool_state.tick_spacing,
        tick_upper_state,
    )?;

    Ok(())
}

/// Validate the seeds for the tick array state
fn validate_tick_state_address<'info>(
    tick_array_address: &Pubkey,
    pool_state: &PoolState,
    tick_index: i32,
) -> Result<()> {
    let expect_start_index =
        TickArrayState::get_array_start_index(tick_index, pool_state.tick_spacing);
    let expected_address = Pubkey::find_program_address(
        &[
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &expect_start_index.to_be_bytes(),
        ],
        &crate::ID,
    )
    .0;
    require_eq!(tick_array_address, &expected_address);
    Ok(())
}

pub fn update_personal_from_protocol(
    personal_position: &mut PersonalPositionState,
    protocol_position: &ProtocolPositionState,
) -> Result<()> {
    personal_position.token_fees_owed_0 = calculate_latest_token_fees(
        personal_position.token_fees_owed_0,
        personal_position.fee_growth_inside_0_last_x64,
        protocol_position.fee_growth_inside_0_last_x64,
        personal_position.liquidity,
    );
    personal_position.token_fees_owed_1 = calculate_latest_token_fees(
        personal_position.token_fees_owed_1,
        personal_position.fee_growth_inside_1_last_x64,
        protocol_position.fee_growth_inside_1_last_x64,
        personal_position.liquidity,
    );

    personal_position.fee_growth_inside_0_last_x64 = protocol_position.fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last_x64 = protocol_position.fee_growth_inside_1_last_x64;

    personal_position.update_rewards(protocol_position.reward_growth_inside, true)?;

    Ok(())
}
