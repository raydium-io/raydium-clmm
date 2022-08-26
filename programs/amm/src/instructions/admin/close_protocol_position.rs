use crate::error::ErrorCode;
use crate::states::*;
use crate::util::close_account;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct CloseProtocolPosition<'info> {
    /// Only admin has the authority to reset initial price
    #[account(mut, address = crate::admin::id())]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_lower_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_lower_state: AccountLoader<'info, TickArrayState>,

    /// CHECK:Account to store data for the position's upper tick
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_upper_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_upper_state: AccountLoader<'info, TickArrayState>,

    #[account(
        mut,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        bump,
    )]
    pub protocol_position_state: Box<Account<'info, ProtocolPositionState>>,
}

pub fn close_protocol_position(
    ctx: Context<CloseProtocolPosition>,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state;
    let protocol_position = &mut ctx.accounts.protocol_position_state;

    check_ticks_order(tick_lower_index, tick_upper_index)?;
    check_tick_array_start_index(
        tick_array_lower_start_index,
        tick_lower_index,
        pool_state.tick_spacing,
    )?;
    check_tick_array_start_index(
        tick_array_upper_start_index,
        tick_upper_index,
        pool_state.tick_spacing,
    )?;
    if protocol_position.tick_lower_index != tick_lower_index
        || protocol_position.tick_upper_index != tick_upper_index
    {
        return err!(ErrorCode::NotApproved);
    }

    // close tick_array_lower account
    pool_state.flip_tick_array_bit(tick_array_lower_start_index)?;
    close_account(
        &ctx.accounts.tick_array_lower_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;
    // close tick_array_upper account
    pool_state.flip_tick_array_bit(tick_array_upper_start_index)?;
    close_account(
        &ctx.accounts.tick_array_upper_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;
    // close protocol_position account
    close_account(
        &ctx.accounts.protocol_position_state.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
    )?;

    Ok(())
}
