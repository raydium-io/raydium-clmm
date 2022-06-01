use super::_modify_position;
use crate::error::ErrorCode;
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

#[derive(Accounts)]
pub struct DecreaseLiquidity<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Decrease liquidity for this position
    #[account(mut)]
    pub personal_position_state: Account<'info, PersonalPositionState>,

    /// The program account acting as the core liquidity custodian for token holder
    pub amm_config: Account<'info, AmmConfig>,

    /// Burn liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
    #[account(mut)]
    pub protocol_position_state: Box<Account<'info, ProcotolPositionState>>,

    /// Account to store data for the position's lower tick
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// Stores init state for the lower tick
    #[account(mut)]
    pub bitmap_lower_state: AccountLoader<'info, TickBitmapState>,

    /// Stores init state for the upper tick
    #[account(mut)]
    pub bitmap_upper_state: AccountLoader<'info, TickBitmapState>,

    /// The latest observation state
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,
}

pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
    liquidity: u64,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    assert!(liquidity > 0);

    let tokens_owed_0_before = ctx.accounts.protocol_position_state.tokens_owed_0;
    let tokens_owed_1_before = ctx.accounts.protocol_position_state.tokens_owed_1;

    let mut core_position_owner = ctx.accounts.amm_config.to_account_info();
    core_position_owner.is_signer = true;
    let mut accounts = BurnParam {
        owner: &Signer::try_from(&core_position_owner)?,
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
        tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
        bitmap_lower_state: &ctx.accounts.bitmap_lower_state,
        bitmap_upper_state: &ctx.accounts.bitmap_upper_state,
        position_state: ctx.accounts.protocol_position_state.as_mut(),
        last_observation_state: ctx.accounts.last_observation_state.as_mut(),
    };

    burn(&mut accounts, ctx.remaining_accounts, liquidity)?;

    let updated_core_position = accounts.position_state;
    let amount_0 = updated_core_position.tokens_owed_0 - tokens_owed_0_before;
    let amount_1 = updated_core_position.tokens_owed_1 - tokens_owed_1_before;
    require!(
        amount_0 >= amount_0_min && amount_1 >= amount_1_min,
        ErrorCode::PriceSlippageCheck
    );

    // Update the tokenized position to the current transaction
    let fee_growth_inside_0_last_x32 = updated_core_position.fee_growth_inside_0_last;
    let fee_growth_inside_1_last_x32 = updated_core_position.fee_growth_inside_1_last;

    let tokenized_position = &mut ctx.accounts.personal_position_state;
    tokenized_position.tokens_owed_0 += amount_0
        + (fee_growth_inside_0_last_x32 - tokenized_position.fee_growth_inside_0_last)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

    tokenized_position.tokens_owed_1 += amount_1
        + (fee_growth_inside_1_last_x32 - tokenized_position.fee_growth_inside_1_last)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

    tokenized_position.fee_growth_inside_0_last = fee_growth_inside_0_last_x32;
    tokenized_position.fee_growth_inside_1_last = fee_growth_inside_1_last_x32;
    tokenized_position.liquidity -= liquidity;

    emit!(DecreaseLiquidityEvent {
        position_nft_mint: tokenized_position.mint,
        liquidity,
        amount_0,
        amount_1
    });

    Ok(())
}

pub struct BurnParam<'b, 'info> {
    /// The position owner
    pub owner: &'b Signer<'info>,

    /// Burn liquidity for this pool
    pub pool_state: &'b mut Account<'info, PoolState>,

    /// The lower tick boundary of the position
    pub tick_lower_state: &'b mut Account<'info, TickState>,

    /// The upper tick boundary of the position
    pub tick_upper_state: &'b mut Account<'info, TickState>,

    /// The bitmap storing initialization state of the lower tick
    pub bitmap_lower_state: &'b AccountLoader<'info, TickBitmapState>,

    /// The bitmap storing initialization state of the upper tick
    pub bitmap_upper_state: &'b AccountLoader<'info, TickBitmapState>,

    /// Burn liquidity from this position
    pub position_state: &'b mut Account<'info, ProcotolPositionState>,

    /// The program account for the most recent oracle observation
    pub last_observation_state: &'b mut Account<'info, ObservationState>,
}

pub fn burn<'b, 'info>(
    ctx: &mut BurnParam<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount: u64,
) -> Result<()> {
    ctx.pool_state.validate_tick_address(
        &ctx.tick_lower_state.key(),
        ctx.tick_lower_state.bump,
        ctx.tick_lower_state.tick,
    )?;
    ctx.pool_state.validate_tick_address(
        &ctx.tick_upper_state.key(),
        ctx.tick_upper_state.bump,
        ctx.tick_upper_state.tick,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_lower_state.key(),
        ctx.bitmap_lower_state.load()?.bump,
        tick_bitmap::position(ctx.tick_lower_state.tick / ctx.pool_state.tick_spacing as i32)
            .word_pos,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_upper_state.key(),
        ctx.bitmap_upper_state.load()?.bump,
        tick_bitmap::position(ctx.tick_upper_state.tick / ctx.pool_state.tick_spacing as i32)
            .word_pos,
    )?;
    ctx.pool_state.validate_position_address(
        &ctx.position_state.key(),
        ctx.position_state.bump,
        &ctx.owner.key(),
        ctx.tick_lower_state.tick,
        ctx.tick_upper_state.tick,
    )?;

    ctx.pool_state.validate_observation_address(
        &ctx.last_observation_state.key(),
        ctx.last_observation_state.bump,
        false,
    )?;

    let (amount_0_int, amount_1_int) = _modify_position(
        -i64::try_from(amount).unwrap(),
        ctx.pool_state,
        ctx.position_state,
        ctx.tick_lower_state,
        ctx.tick_upper_state,
        ctx.bitmap_lower_state,
        ctx.bitmap_upper_state,
        ctx.last_observation_state,
        remaining_accounts,
    )?;

    let amount_0 = (-amount_0_int) as u64;
    let amount_1 = (-amount_1_int) as u64;
    if amount_0 > 0 || amount_1 > 0 {
        let position_state = &mut ctx.position_state;
        position_state.tokens_owed_0 += amount_0;
        position_state.tokens_owed_1 += amount_1;
    }

    Ok(())
}
