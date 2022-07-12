use super::_modify_position;
use crate::error::ErrorCode;
use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv};
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct DecreaseLiquidity<'info> {
    /// The position owner or delegated authority
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Decrease liquidity for this position
    #[account(mut)]
    pub personal_position: Account<'info, PersonalPositionState>,

    /// The program account acting as the core liquidity custodian for token holder
    pub amm_config: Account<'info, AmmConfig>,

    /// Burn liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
    #[account(mut)]
    pub protocol_position: Box<Account<'info, ProcotolPositionState>>,

    /// Token_0 vault
    #[account(
        mut,
        constraint = pool_state.token_vault_0 == token_vault_0.key()
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// Token_1 vault
    #[account(
        mut,
        constraint = pool_state.token_vault_1 == token_vault_1.key()
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

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

    /// The latest observation state
    #[account(mut)]
    pub last_observation: Box<Account<'info, ObservationState>>,

    /// The next observation state
    #[account(mut)]
    pub next_observation: Box<Account<'info, ObservationState>>,

    /// The destination token account for the collected amount_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub recipient_token_account_0: Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_1
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub recipient_token_account_1: Account<'info, TokenAccount>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    assert!(liquidity > 0);

    let mut procotol_position_owner = ctx.accounts.amm_config.to_account_info();
    procotol_position_owner.is_signer = true;
    let mut pool_state = ctx.accounts.pool_state.as_mut().clone();
    let mut accounts = BurnParam {
        owner: &Signer::try_from(&procotol_position_owner)?,
        pool_state: &mut pool_state,
        tick_lower_state: ctx.accounts.tick_lower.as_mut(),
        tick_upper_state: ctx.accounts.tick_upper.as_mut(),
        bitmap_lower_state: &ctx.accounts.tick_bitmap_lower,
        bitmap_upper_state: &ctx.accounts.tick_bitmap_upper,
        procotol_position_state: ctx.accounts.protocol_position.as_mut(),
        last_observation_state: ctx.accounts.last_observation.as_mut(),
        next_observation_state: ctx.accounts.next_observation.as_mut(),
    };

    let (decrease_amount_0, decrease_amount_1) =
        burn(&mut accounts, ctx.remaining_accounts, liquidity)?;
    require!(
        decrease_amount_0 >= amount_0_min && decrease_amount_1 >= amount_1_min,
        ErrorCode::PriceSlippageCheck
    );

    if decrease_amount_0 > 0 {
        #[cfg(feature = "enable-log")]
        msg!(
            "decrease_amount_0, vault_0 balance: {}, recipient_token_account balance before transfer:{}, tranafer amount:{}",
            ctx.accounts.token_vault_0.amount,
            ctx.accounts.recipient_token_account_0.amount,
            decrease_amount_0,
        );
        transfer_from_pool_vault_to_user(
            ctx.accounts.pool_state.clone().as_mut(),
            &ctx.accounts.token_vault_0,
            &ctx.accounts.recipient_token_account_0,
            &ctx.accounts.token_program,
            decrease_amount_0,
        )?;
    }
    if decrease_amount_1 > 0 {
        #[cfg(feature = "enable-log")]
        msg!(
            "decrease_amount_1, vault_1 balance: {}, recipient_token_account balance before transfer:{}, tranafer amount:{}",
            ctx.accounts.token_vault_1.amount,
            ctx.accounts.recipient_token_account_1.amount,
            decrease_amount_1,
        );
        transfer_from_pool_vault_to_user(
            ctx.accounts.pool_state.clone().as_mut(),
            &ctx.accounts.token_vault_1,
            &ctx.accounts.recipient_token_account_1,
            &ctx.accounts.token_program,
            decrease_amount_1,
        )?;
    }

    // Update the tokenized position to the current transaction
    let updated_procotol_position = accounts.procotol_position_state;
    let fee_growth_inside_0_last_x64 = updated_procotol_position.fee_growth_inside_0_last;
    let fee_growth_inside_1_last_x64 = updated_procotol_position.fee_growth_inside_1_last;
    let personal_position = &mut ctx.accounts.personal_position;

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

    // update rewards, must update before decrease liquidity
    personal_position.update_rewards(updated_procotol_position.reward_growth_inside)?;
    personal_position.liquidity = personal_position.liquidity.checked_sub(liquidity).unwrap();

    emit!(DecreaseLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        amount_0: decrease_amount_0,
        amount_1: decrease_amount_1
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
    pub procotol_position_state: &'b mut Account<'info, ProcotolPositionState>,

    /// The program account for the most recent oracle observation
    pub last_observation_state: &'b mut Account<'info, ObservationState>,

    /// The program account for the most recent oracle observation
    pub next_observation_state: &'b mut Account<'info, ObservationState>,
}

pub fn burn<'b, 'info>(
    ctx: &mut BurnParam<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    liquidity: u128,
) -> Result<(u64, u64)> {
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
    ctx.pool_state.validate_protocol_position_address(
        &ctx.procotol_position_state.key(),
        ctx.procotol_position_state.bump,
        ctx.tick_lower_state.tick,
        ctx.tick_upper_state.tick,
    )?;

    ctx.pool_state.validate_observation_address(
        &ctx.last_observation_state.key(),
        ctx.last_observation_state.bump,
        false,
    )?;

    let (amount_0_int, amount_1_int) = _modify_position(
        -i128::try_from(liquidity).unwrap(),
        ctx.pool_state,
        ctx.procotol_position_state,
        ctx.tick_lower_state,
        ctx.tick_upper_state,
        ctx.bitmap_lower_state,
        ctx.bitmap_upper_state,
        ctx.last_observation_state,
        ctx.next_observation_state,
        remaining_accounts,
    )?;

    let amount_0 = (-amount_0_int) as u64;
    let amount_1 = (-amount_1_int) as u64;

    Ok((amount_0, amount_1))
}
