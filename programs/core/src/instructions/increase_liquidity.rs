use super::{add_liquidity, MintParam};
use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct IncreaseLiquidity<'info> {
    /// Pays to mint the position
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Authority PDA for the NFT mint
    pub amm_config: Account<'info, AmmConfig>,

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
    #[account(mut)]
    pub protocol_position: Box<Account<'info, ProcotolPositionState>>,

    /// Increase liquidity for this position
    #[account(mut)]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Account to store data for the position's lower tick
    #[account(mut)]
    pub tick_lower: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_upper: Box<Account<'info, TickState>>,

    /// Stores init state for the lower tick
    #[account(mut)]
    pub tick_bitmap_lower: AccountLoader<'info, TickBitmapState>,

    /// Stores init state for the upper tick
    #[account(mut)]
    pub tick_bitmap_upper: AccountLoader<'info, TickBitmapState>,

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
    pub last_observation: Box<Account<'info, ObservationState>>,

    /// The next observation state
    #[account(mut)]
    pub next_observation: Box<Account<'info, ObservationState>>,
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
    let tick_lower = ctx.accounts.tick_lower.tick;
    let tick_upper = ctx.accounts.tick_upper.tick;
    let mut accounts = MintParam {
        payer: &ctx.accounts.nft_owner,
        token_account_0: ctx.accounts.token_account_0.as_mut(),
        token_account_1: ctx.accounts.token_account_1.as_mut(),
        token_vault_0: ctx.accounts.token_vault_0.as_mut(),
        token_vault_1: ctx.accounts.token_vault_1.as_mut(),
        protocol_position_owner: UncheckedAccount::try_from(
            ctx.accounts.amm_config.to_account_info(),
        ),
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower: ctx.accounts.tick_lower.as_mut(),
        tick_upper: ctx.accounts.tick_upper.as_mut(),
        bitmap_lower: &ctx.accounts.tick_bitmap_lower,
        bitmap_upper: &ctx.accounts.tick_bitmap_upper,
        protocol_position: ctx.accounts.protocol_position.as_mut(),
        last_observation: ctx.accounts.last_observation.as_mut(),
        next_observation: ctx.accounts.next_observation.as_mut(),
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

    let updated_procotol_position = accounts.protocol_position;
    let fee_growth_inside_0_last_x64 = updated_procotol_position.fee_growth_inside_0_last;
    let fee_growth_inside_1_last_x64 = updated_procotol_position.fee_growth_inside_1_last;

    // Update tokenized position metadata
    let personal_position = ctx.accounts.personal_position.as_mut();
    personal_position.token_fees_owed_0 = personal_position
        .token_fees_owed_0
        .checked_add(
            U128::from(
                fee_growth_inside_0_last_x64
                    .saturating_sub(personal_position.fee_growth_inside_0_last),
            )
            .mul_div_floor(
                U128::from(personal_position.liquidity),
                U128::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64(),
        )
        .unwrap();

    personal_position.token_fees_owed_1 = personal_position
        .token_fees_owed_1
        .checked_add(
            U128::from(
                fee_growth_inside_1_last_x64
                    .saturating_sub(personal_position.fee_growth_inside_1_last),
            )
            .mul_div_floor(
                U128::from(personal_position.liquidity),
                U128::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64(),
        )
        .unwrap();

    personal_position.fee_growth_inside_0_last = fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last = fee_growth_inside_1_last_x64;

    // update rewards, must update before increase liquidity
    personal_position.update_rewards(updated_procotol_position.reward_growth_inside)?;
    personal_position.liquidity = personal_position.liquidity.checked_add(liquidity).unwrap();

    emit!(IncreaseLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        amount_0,
        amount_1
    });

    Ok(())
}
