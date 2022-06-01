use super::{burn, BurnParam};
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::Deref;

pub struct CollectParam<'b, 'info> {
    /// The position owner
    pub owner: &'b Signer<'info>,

    /// The program account for the liquidity pool from which fees are collected
    pub pool_state: &'b mut Account<'info, PoolState>,

    /// The lower tick of the position for which to collect fees
    pub tick_lower_state: &'b mut Account<'info, TickState>,

    /// The upper tick of the position for which to collect fees
    pub tick_upper_state: &'b mut Account<'info, TickState>,

    /// The position program account to collect fees from
    pub position_state: &'b mut Account<'info, ProcotolPositionState>,

    /// The address that holds pool tokens for token_0
    pub vault_0: &'b mut Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_1
    pub vault_1: &'b mut Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_0
    pub recipient_wallet_0: &'b mut Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_1
    pub recipient_wallet_1: &'b mut Account<'info, TokenAccount>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CollectFee<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// The program account of the NFT for which tokens are being collected
    #[account(mut)]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

    /// The program account acting as the core liquidity custodian for token holder
    pub factory_state: Box<Account<'info, FactoryState>>,

    /// The program account for the liquidity pool from which fees are collected
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The program account to access the core program position state
    #[account(mut)]
    pub protocol_position_state: Box<Account<'info, ProcotolPositionState>>,

    /// The program account for the position's lower tick
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The program account for the position's upper tick
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The bitmap program account for the init state of the lower tick
    #[account(mut)]
    pub bitmap_lower_state: AccountLoader<'info, TickBitmapState>,

    /// Stores init state for the upper tick
    #[account(mut)]
    pub bitmap_upper_state: AccountLoader<'info, TickBitmapState>,

    /// The latest observation state
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// The pool's token account for token_0
    #[account(mut)]
    pub token_vault_0: Account<'info, TokenAccount>,

    /// The pool's token account for token_1
    #[account(mut)]
    pub token_vault_1: Account<'info, TokenAccount>,

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

pub fn collect_fee<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CollectFee<'info>>,
    amount_0_max: u64,
    amount_1_max: u64,
) -> Result<()> {
    assert!(amount_0_max > 0 || amount_1_max > 0);

    let tokenized_position = ctx.accounts.personal_position_state.as_mut();
    let mut tokens_owed_0 = tokenized_position.tokens_owed_0;
    let mut tokens_owed_1 = tokenized_position.tokens_owed_1;

    let mut core_position_owner = ctx.accounts.factory_state.to_account_info();
    core_position_owner.is_signer = true;
    // trigger an update of the position fees owed and fee growth snapshots if it has any liquidity
    if tokenized_position.liquidity > 0 {
        let mut burn_accounts = BurnParam {
            owner: &Signer::try_from(&core_position_owner)?,
            pool_state: ctx.accounts.pool_state.as_mut(),
            tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
            tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
            bitmap_lower_state: &ctx.accounts.bitmap_lower_state,
            bitmap_upper_state: &ctx.accounts.bitmap_upper_state,
            position_state: ctx.accounts.protocol_position_state.as_mut(),
            last_observation_state: ctx.accounts.last_observation_state.as_mut(),
        };
        burn(&mut burn_accounts, ctx.remaining_accounts, 0)?;

        let core_position = burn_accounts.position_state.deref();

        tokens_owed_0 += (core_position.fee_growth_inside_0_last
            - tokenized_position.fee_growth_inside_0_last)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();
        tokens_owed_1 += (core_position.fee_growth_inside_1_last
            - tokenized_position.fee_growth_inside_1_last)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

        tokenized_position.fee_growth_inside_0_last = core_position.fee_growth_inside_0_last;
        tokenized_position.fee_growth_inside_1_last = core_position.fee_growth_inside_1_last;
    }

    // adjust amounts to the max for the position
    let amount_0 = amount_0_max.min(tokens_owed_0);
    let amount_1 = amount_1_max.min(tokens_owed_1);

    // let mut core_position_owner = ctx.accounts.factory_state.to_account_info().clone();
    // core_position_owner.is_signer = true;

    msg!("withdrawing amounts {} {}", amount_0, amount_1);
    msg!(
        "vault balances {} {}",
        ctx.accounts.token_vault_0.amount,
        ctx.accounts.token_vault_1.amount
    );

    let mut accounts = CollectParam {
        owner: &Signer::try_from(&core_position_owner)?,
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
        tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
        position_state: ctx.accounts.protocol_position_state.as_mut(),
        vault_0: &mut ctx.accounts.token_vault_0,
        vault_1: &mut ctx.accounts.token_vault_1,
        recipient_wallet_0: &mut ctx.accounts.recipient_token_account_0,
        recipient_wallet_1: &mut ctx.accounts.recipient_token_account_1,
        token_program: ctx.accounts.token_program.clone(),
    };
    collect(&mut accounts, amount_0, amount_1)?;

    // sometimes there will be a few less wei than expected due to rounding down in core, but
    // we just subtract the full amount expected
    // instead of the actual amount so we can burn the token
    tokenized_position.tokens_owed_0 = tokens_owed_0 - amount_0;
    tokenized_position.tokens_owed_1 = tokens_owed_1 - amount_1;

    emit!(CollectPersonalFeeEvent {
        position_nft_mint: tokenized_position.mint,
        recipient_token_account_0: ctx.accounts.recipient_token_account_0.key(),
        recipient_token_account_1: ctx.accounts.recipient_token_account_1.key(),
        amount_0,
        amount_1
    });

    Ok(())
}

pub fn collect<'b, 'info>(
    ctx: &mut CollectParam<'b, 'info>,
    amount_0_requested: u64,
    amount_1_requested: u64,
) -> Result<()> {
    let pool_state_info = ctx.pool_state.to_account_info();

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

    ctx.pool_state.validate_position_address(
        &ctx.position_state.key(),
        ctx.position_state.bump,
        &ctx.owner.key(),
        ctx.tick_lower_state.tick,
        ctx.tick_upper_state.tick,
    )?;

    let position = &mut ctx.position_state;

    let amount_0 = amount_0_requested.min(position.tokens_owed_0);
    let amount_1 = amount_1_requested.min(position.tokens_owed_1);

    if amount_0 > 0 {
        position.tokens_owed_0 -= amount_0;
        transfer_from_pool_vault_to_user(
            ctx.pool_state,
            &ctx.vault_0,
            &ctx.recipient_wallet_0,
            &ctx.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        position.tokens_owed_1 -= amount_1;
        transfer_from_pool_vault_to_user(
            ctx.pool_state,
            &ctx.vault_1,
            &ctx.recipient_wallet_1,
            &ctx.token_program,
            amount_1,
        )?;
    }

    emit!(CollectFeeEvent {
        pool_state: pool_state_info.key(),
        owner: ctx.owner.key(),
        tick_lower: ctx.tick_lower_state.tick,
        tick_upper: ctx.tick_upper_state.tick,
        collect_amount_0: amount_0,
        collect_amount_1: amount_1,
    });

    Ok(())
}
