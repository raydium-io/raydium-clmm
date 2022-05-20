use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token::{Token, TokenAccount};
use std::ops::Deref;

#[derive(Accounts)]
pub struct CollectContext<'info> {
    /// The position owner
    pub owner: Signer<'info>,

    /// The program account for the liquidity pool from which fees are collected
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// The lower tick of the position for which to collect fees
    /// CHECK: Safety check performed inside function body
    pub tick_lower_state: UncheckedAccount<'info>,

    /// The upper tick of the position for which to collect fees
    /// CHECK: Safety check performed inside function body
    pub tick_upper_state: UncheckedAccount<'info>,

    /// The position program account to collect fees from
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub position_state: UncheckedAccount<'info>,

    /// The account holding pool tokens for token_0
    #[account(mut)]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The account holding pool tokens for token_1
    #[account(mut)]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// The destination token account for the collected amount_0
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub recipient_wallet_0: UncheckedAccount<'info>,

    /// The destination token account for the collected amount_1
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub recipient_wallet_1: UncheckedAccount<'info>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn collect(
    ctx: Context<CollectContext>,
    amount_0_requested: u64,
    amount_1_requested: u64,
) -> Result<()> {
    let pool_state =
        AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
    let mut pool = pool_state.load_mut()?;

    let tick_lower_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
    let tick_lower = *tick_lower_state.load()?.deref();
    pool.validate_tick_address(
        &ctx.accounts.tick_lower_state.key(),
        tick_lower.bump,
        tick_lower.tick,
    )?;

    let tick_upper_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
    let tick_upper = *tick_upper_state.load()?.deref();
    pool.validate_tick_address(
        &ctx.accounts.tick_upper_state.key(),
        tick_upper.bump,
        tick_upper.tick,
    )?;

    let position_state =
        AccountLoader::<PositionState>::try_from(&ctx.accounts.position_state.to_account_info())?;
    pool.validate_position_address(
        &ctx.accounts.position_state.key(),
        position_state.load()?.bump,
        &ctx.accounts.owner.key(),
        tick_lower.tick,
        tick_upper.tick,
    )?;

    require!(pool.unlocked, ErrorCode::LOK);
    pool.unlocked = false;

    let mut position = position_state.load_mut()?;

    let amount_0 = amount_0_requested.min(position.tokens_owed_0);
    let amount_1 = amount_1_requested.min(position.tokens_owed_1);

    let pool_state_seeds = [
        &POOL_SEED.as_bytes(),
        &pool.token_0.to_bytes() as &[u8],
        &pool.token_1.to_bytes() as &[u8],
        &pool.fee.to_be_bytes(),
        &[pool.bump],
    ];

    drop(pool);
    if amount_0 > 0 {
        position.tokens_owed_0 -= amount_0;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(),
                token::Transfer {
                    from: ctx.accounts.vault_0.to_account_info().clone(),
                    to: ctx.accounts.recipient_wallet_0.to_account_info().clone(),
                    authority: pool_state.to_account_info().clone(),
                },
                &[&pool_state_seeds[..]],
            ),
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        position.tokens_owed_1 -= amount_1;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(),
                token::Transfer {
                    from: ctx.accounts.vault_1.to_account_info().clone(),
                    to: ctx.accounts.recipient_wallet_1.to_account_info().clone(),
                    authority: pool_state.to_account_info().clone(),
                },
                &[&pool_state_seeds[..]],
            ),
            amount_1,
        )?;
    }

    emit!(CollectEvent {
        pool_state: pool_state.key(),
        owner: ctx.accounts.owner.key(),
        tick_lower: tick_lower.tick,
        tick_upper: tick_upper.tick,
        amount_0,
        amount_1,
    });

    pool_state.load_mut()?.unlocked = true;
    Ok(())
}
