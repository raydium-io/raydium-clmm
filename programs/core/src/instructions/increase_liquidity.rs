use super::{add_liquidity, MintParam};
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct IncreaseLiquidity<'info> {
    /// Pays to mint the position
    pub payer: Signer<'info>,

    /// Authority PDA for the NFT mint
    pub factory_state: Account<'info, FactoryState>,

    /// Increase liquidity for this position
    #[account(mut)]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

    /// Mint liquidity for this pool
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

    /// The payer's token account for token_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<Account<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// The latest observation state
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
}

pub fn increase_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    let tick_lower = ctx.accounts.tick_lower_state.tick;
    let tick_upper = ctx.accounts.tick_upper_state.tick;
    let mut accounts = MintParam {
        minter: &ctx.accounts.payer,
        token_account_0: ctx.accounts.token_account_0.as_mut(),
        token_account_1: ctx.accounts.token_account_1.as_mut(),
        token_vault_0: ctx.accounts.token_vault_0.as_mut(),
        token_vault_1: ctx.accounts.token_vault_1.as_mut(),
        protocol_position_owner: UncheckedAccount::try_from(ctx.accounts.factory_state.to_account_info()),
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
        tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
        bitmap_lower_state: &ctx.accounts.bitmap_lower_state,
        bitmap_upper_state: &ctx.accounts.bitmap_upper_state,
        position_state: ctx.accounts.protocol_position_state.as_mut(),
        last_observation_state: ctx.accounts.last_observation_state.as_mut(),
        token_program: ctx.accounts.token_program.clone(),
    };

    let (liquidity, amount_0, amount_1) = add_liquidity(
        &mut accounts,
        ctx.remaining_accounts,
        amount_0_desired,
        amount_1_desired,
        amount_0_min,
        amount_1_min,
        tick_lower,
        tick_upper,
    )?;

    let updated_core_position = accounts.position_state;
    let fee_growth_inside_0_last_x32 = updated_core_position.fee_growth_inside_0_last_x32;
    let fee_growth_inside_1_last_x32 = updated_core_position.fee_growth_inside_1_last_x32;

    // Update tokenized position metadata
    let position = ctx.accounts.personal_position_state.as_mut();
    position.tokens_owed_0 += (fee_growth_inside_0_last_x32
        - position.fee_growth_inside_0_last_x32)
        .mul_div_floor(position.liquidity, fixed_point_32::Q32)
        .unwrap();

    position.tokens_owed_1 += (fee_growth_inside_1_last_x32
        - position.fee_growth_inside_1_last_x32)
        .mul_div_floor(position.liquidity, fixed_point_32::Q32)
        .unwrap();

    position.fee_growth_inside_0_last_x32 = fee_growth_inside_0_last_x32;
    position.fee_growth_inside_1_last_x32 = fee_growth_inside_1_last_x32;
    position.liquidity += liquidity;

    emit!(IncreaseLiquidityEvent {
        token_id: position.mint,
        liquidity,
        amount_0,
        amount_1
    });

    Ok(())
}
