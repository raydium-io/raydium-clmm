use super::{burn, collect, BurnContext, CollectContext};
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::program::AmmCore;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::collections::BTreeMap;
use std::ops::Deref;

#[derive(Accounts)]
pub struct CollectFromTokenized<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == tokenized_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// The program account of the NFT for which tokens are being collected
    #[account(mut)]
    pub tokenized_position_state: Box<Account<'info, TokenizedPositionState>>,

    /// The program account acting as the core liquidity custodian for token holder
    pub factory_state: Box<Account<'info, FactoryState>>,

    /// The program account for the liquidity pool from which fees are collected
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The program account to access the core program position state
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub core_position_state: Box<Account<'info, PositionState>>,

    /// The program account for the position's lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The program account for the position's upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The bitmap program account for the init state of the lower tick
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

    /// The pool's token account for token_0
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub vault_0: Account<'info, TokenAccount>,

    /// The pool's token account for token_1
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub vault_1: Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_0
    #[account(
        mut,
        token::mint = vault_0.mint
    )]
    pub recipient_wallet_0: Account<'info, TokenAccount>,

    /// The destination token account for the collected amount_1
    #[account(
        mut,
        token::mint = vault_1.mint
    )]
    pub recipient_wallet_1: Account<'info, TokenAccount>,

    /// The core program where liquidity is burned
    pub core_program: Program<'info, AmmCore>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn collect_from_tokenized<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CollectFromTokenized<'info>>,
    amount_0_max: u64,
    amount_1_max: u64,
) -> Result<()> {
    assert!(amount_0_max > 0 || amount_1_max > 0);

    let tokenized_position = ctx.accounts.tokenized_position_state.as_mut();
    let mut tokens_owed_0 = tokenized_position.tokens_owed_0;
    let mut tokens_owed_1 = tokenized_position.tokens_owed_1;

    // trigger an update of the position fees owed and fee growth snapshots if it has any liquidity
    if tokenized_position.liquidity > 0 {
        let mut core_position_owner = ctx.accounts.factory_state.to_account_info();
        core_position_owner.is_signer = true;
        let mut burn_accounts = BurnContext {
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
                &mut burn_accounts,
                ctx.remaining_accounts,
                BTreeMap::default(),
            ),
            0,
        )?;

        let core_position = burn_accounts.position_state.deref();

        tokens_owed_0 += (core_position.fee_growth_inside_0_last_x32
            - tokenized_position.fee_growth_inside_0_last_x32)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();
        tokens_owed_1 += (core_position.fee_growth_inside_1_last_x32
            - tokenized_position.fee_growth_inside_1_last_x32)
            .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
            .unwrap();

        tokenized_position.fee_growth_inside_0_last_x32 =
            core_position.fee_growth_inside_0_last_x32;
        tokenized_position.fee_growth_inside_1_last_x32 =
            core_position.fee_growth_inside_1_last_x32;
    }

    // adjust amounts to the max for the position
    let amount_0 = amount_0_max.min(tokens_owed_0);
    let amount_1 = amount_1_max.min(tokens_owed_1);

    let mut core_position_owner = ctx.accounts.factory_state.to_account_info().clone();
    core_position_owner.is_signer = true;

    msg!("withdrawing amounts {} {}", amount_0, amount_1);
    msg!(
        "vault balances {} {}",
        ctx.accounts.vault_0.amount,
        ctx.accounts.vault_1.amount
    );

    let mut accounts = CollectContext {
        owner: Signer::try_from(&core_position_owner)?,
        pool_state: ctx.accounts.pool_state.clone(),
        tick_lower_state: ctx.accounts.tick_lower_state.clone(),
        tick_upper_state: ctx.accounts.tick_upper_state.clone(),
        position_state: ctx.accounts.core_position_state.clone(),
        vault_0: ctx.accounts.vault_0.clone(),
        vault_1: ctx.accounts.vault_1.clone(),
        recipient_wallet_0: ctx.accounts.recipient_wallet_0.clone(),
        recipient_wallet_1: ctx.accounts.recipient_wallet_1.clone(),
        token_program: ctx.accounts.token_program.clone(),
    };
    collect(
        Context::new(&crate::id(), &mut accounts, &[], BTreeMap::default()),
        amount_0,
        amount_1,
    )?;

    // sometimes there will be a few less wei than expected due to rounding down in core, but
    // we just subtract the full amount expected
    // instead of the actual amount so we can burn the token
    tokenized_position.tokens_owed_0 = tokens_owed_0 - amount_0;
    tokenized_position.tokens_owed_1 = tokens_owed_1 - amount_1;

    emit!(CollectTokenizedEvent {
        token_id: tokenized_position.mint,
        recipient_wallet_0: ctx.accounts.recipient_wallet_0.key(),
        recipient_wallet_1: ctx.accounts.recipient_wallet_1.key(),
        amount_0,
        amount_1
    });

    Ok(())
}
