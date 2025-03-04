use crate::states::*;
use anchor_lang::prelude::*;

use super::modify_position;

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

    // Settle fees and rewards for this position without changing liquidity: same path as
    // increase/decrease_liquidity with a zero delta.
    let result = modify_position(
        0,
        &mut pool_state,
        &mut tick_lower_state,
        &mut tick_upper_state,
        timestamp,
    )?;
    let (tick_lower_index, tick_upper_index) = (
        protocol_position.tick_lower_index,
        protocol_position.tick_upper_index,
    );
    protocol_position.update(
        tick_lower_index,
        tick_upper_index,
        0,
        result.fee_growth_inside_0_x64,
        result.fee_growth_inside_1_x64,
        result.reward_growths_inside,
    )?;
    personal_position.increase_liquidity(
        0,
        result.fee_growth_inside_0_x64,
        result.fee_growth_inside_1_x64,
        result.reward_growths_inside,
        clock.epoch,
    )?;

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
