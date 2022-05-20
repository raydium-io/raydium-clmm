use super::{add_liquidity, MintContext};
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::program::AmmCore;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct IncreaseLiquidity<'info> {
    /// Pays to mint the position
    pub payer: Signer<'info>,

    /// Authority PDA for the NFT mint
    pub factory_state: AccountLoader<'info, FactoryState>,

    /// Increase liquidity for this position
    #[account(mut)]
    pub tokenized_position_state: AccountLoader<'info, TokenizedPositionState>,

    /// Mint liquidity for this pool
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// Core program account to store position data
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub core_position_state: UncheckedAccount<'info>,

    /// Account to store data for the position's lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_lower_state: UncheckedAccount<'info>,

    /// Account to store data for the position's upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_upper_state: UncheckedAccount<'info>,

    /// Stores init state for the lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_lower_state: UncheckedAccount<'info>,

    /// Stores init state for the upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_upper_state: UncheckedAccount<'info>,

    /// The payer's token account for token_0
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub token_account_0: UncheckedAccount<'info>,

    /// The payer's token account for token_1
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub token_account_1: UncheckedAccount<'info>,

    /// The pool's token account for token_0
    #[account(mut)]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The pool's token account for token_1
    #[account(mut)]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// The latest observation state
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: UncheckedAccount<'info>,

    /// The core program where liquidity is minted
    pub core_program: Program<'info, AmmCore>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
}

pub fn increase_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    deadline: i64,
) -> Result<()> {
    let pool_state =
        AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
    let tick_lower_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
    let tick_lower = tick_lower_state.load()?.tick;

    let tick_upper_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
    let tick_upper = tick_upper_state.load()?.tick;

    let mut accs = MintContext {
        minter: ctx.accounts.payer.clone(),
        token_account_0: ctx.accounts.token_account_0.clone(),
        token_account_1: ctx.accounts.token_account_1.clone(),
        vault_0: ctx.accounts.vault_0.clone(),
        vault_1: ctx.accounts.vault_1.clone(),
        recipient: UncheckedAccount::try_from(ctx.accounts.factory_state.to_account_info()),
        pool_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower_state: ctx.accounts.bitmap_lower_state.clone(),
        bitmap_upper_state: ctx.accounts.bitmap_upper_state.clone(),
        position_state: ctx.accounts.core_position_state.clone(),
        last_observation_state: ctx.accounts.last_observation_state.clone(),
        token_program: ctx.accounts.token_program.clone(),
        callback_handler: UncheckedAccount::try_from(ctx.accounts.core_program.to_account_info()),
    };

    let (liquidity, amount_0, amount_1) = add_liquidity(
        &mut accs,
        ctx.remaining_accounts,
        amount_0_desired,
        amount_1_desired,
        amount_0_min,
        amount_1_min,
        tick_lower,
        tick_upper,
    )?;

    let core_position_state = AccountLoader::<PositionState>::try_from(
        &ctx.accounts.core_position_state.to_account_info(),
    )?;
    let fee_growth_inside_0_last_x32 = core_position_state.load()?.fee_growth_inside_0_last_x32;
    let fee_growth_inside_1_last_x32 = core_position_state.load()?.fee_growth_inside_1_last_x32;

    // Update tokenized position metadata
    let mut position = ctx.accounts.tokenized_position_state.load_mut()?;
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
