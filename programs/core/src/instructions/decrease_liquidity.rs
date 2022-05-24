use super::{burn, BurnContext};
use crate::error::ErrorCode;
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::program::AmmCore;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use std::collections::BTreeMap;

#[derive(Accounts)]
pub struct DecreaseLiquidity<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == tokenized_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Decrease liquidity for this position
    #[account(mut)]
    pub tokenized_position_state: Account<'info, TokenizedPositionState>,

    /// The program account acting as the core liquidity custodian for token holder
    pub factory_state: Account<'info, FactoryState>,

    /// Burn liquidity for this pool
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub core_position_state: Box<Account<'info, PositionState>>,

    /// Account to store data for the position's lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// Stores init state for the lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_lower_state: Box<Account<'info, TickBitmapState>>,

    /// Stores init state for the upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_upper_state: Box<Account<'info, TickBitmapState>>,

    /// The latest observation state
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// The core program where liquidity is burned
    pub core_program: Program<'info, AmmCore>,
}

pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
    liquidity: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    deadline: i64,
) -> Result<()> {
    assert!(liquidity > 0);

    let tokens_owed_0_before = ctx.accounts.core_position_state.tokens_owed_0;
    let tokens_owed_1_before = ctx.accounts.core_position_state.tokens_owed_1;

    let mut core_position_owner = ctx.accounts.factory_state.to_account_info();
    core_position_owner.is_signer = true;
    let mut accounts = BurnContext {
        owner: Signer::try_from(&core_position_owner)?,
        pool_state: ctx.accounts.pool_state.clone(),
        tick_lower_state: ctx.accounts.tick_lower_state.clone(),
        tick_upper_state: ctx.accounts.tick_upper_state.clone(),
        bitmap_lower_state: ctx.accounts.bitmap_lower_state.clone(),
        bitmap_upper_state: ctx.accounts.bitmap_upper_state.clone(),
        position_state: ctx.accounts.core_position_state.clone(),
        last_observation_state: ctx.accounts.last_observation_state.clone(),
    };
    burn(
        Context::new(
            &crate::id(),
            &mut accounts,
            ctx.remaining_accounts,
            BTreeMap::default(),
        ),
        liquidity,
    )?;
    let updated_core_position = accounts.position_state;
    let amount_0 = updated_core_position.tokens_owed_0 - tokens_owed_0_before;
    let amount_1 = updated_core_position.tokens_owed_1 - tokens_owed_1_before;
    require!(
        amount_0 >= amount_0_min && amount_1 >= amount_1_min,
        ErrorCode::PriceSlippageCheck
    );

    // Update the tokenized position to the current transaction
    let fee_growth_inside_0_last_x32 = updated_core_position.fee_growth_inside_0_last_x32;
    let fee_growth_inside_1_last_x32 = updated_core_position.fee_growth_inside_1_last_x32;

    let tokenized_position = &mut ctx.accounts.tokenized_position_state;
    tokenized_position.tokens_owed_0 += amount_0
        + (fee_growth_inside_0_last_x32 - tokenized_position.fee_growth_inside_0_last_x32)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

    tokenized_position.tokens_owed_1 += amount_1
        + (fee_growth_inside_1_last_x32 - tokenized_position.fee_growth_inside_1_last_x32)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

    tokenized_position.fee_growth_inside_0_last_x32 = fee_growth_inside_0_last_x32;
    tokenized_position.fee_growth_inside_1_last_x32 = fee_growth_inside_1_last_x32;
    tokenized_position.liquidity -= liquidity;

    emit!(DecreaseLiquidityEvent {
        token_id: tokenized_position.mint,
        liquidity,
        amount_0,
        amount_1
    });

    Ok(())
}
