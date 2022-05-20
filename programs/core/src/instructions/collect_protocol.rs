use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct CollectProtocol<'info> {
    /// Valid protocol owner
    #[account(address = factory_state.load()?.owner)]
    pub owner: Signer<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub factory_state: AccountLoader<'info, FactoryState>,

    /// Pool state stores accumulated protocol fee amount
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = vault_0.key() == get_associated_token_address(&pool_state.key(), &pool_state.load()?.token_0),
    )]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = vault_1.key() == get_associated_token_address(&pool_state.key(), &pool_state.load()?.token_1),
    )]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// The address that receives the collected token_0 protocol fees
    #[account(mut)]
    pub recipient_wallet_0: Box<Account<'info, TokenAccount>>,

    /// The address that receives the collected token_1 protocol fees
    #[account(mut)]
    pub recipient_wallet_1: Box<Account<'info, TokenAccount>>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

pub fn collect_protocol(
    ctx: Context<CollectProtocol>,
    amount_0_requested: u64,
    amount_1_requested: u64,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    require!(pool_state.unlocked, ErrorCode::LOK);
    pool_state.unlocked = false;

    let amount_0 = amount_0_requested.min(pool_state.protocol_fees_token_0);
    let amount_1 = amount_1_requested.min(pool_state.protocol_fees_token_1);

    let pool_state_seeds = [
        &POOL_SEED.as_bytes(),
        &pool_state.token_0.to_bytes() as &[u8],
        &pool_state.token_1.to_bytes() as &[u8],
        &pool_state.fee.to_be_bytes(),
        &[pool_state.bump],
    ];

    pool_state.protocol_fees_token_0 -= amount_0;
    pool_state.protocol_fees_token_1 -= amount_1;
    drop(pool_state);

    if amount_0 > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(),
                token::Transfer {
                    from: ctx.accounts.vault_0.to_account_info().clone(),
                    to: ctx.accounts.recipient_wallet_0.to_account_info().clone(),
                    authority: ctx.accounts.pool_state.to_account_info().clone(),
                },
                &[&pool_state_seeds[..]],
            ),
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(),
                token::Transfer {
                    from: ctx.accounts.vault_1.to_account_info().clone(),
                    to: ctx.accounts.recipient_wallet_1.to_account_info().clone(),
                    authority: ctx.accounts.pool_state.to_account_info().clone(),
                },
                &[&pool_state_seeds[..]],
            ),
            amount_1,
        )?;
    }

    emit!(CollectProtocolEvent {
        pool_state: ctx.accounts.pool_state.key(),
        sender: ctx.accounts.owner.key(),
        recipient_wallet_0: ctx.accounts.recipient_wallet_0.key(),
        recipient_wallet_1: ctx.accounts.recipient_wallet_1.key(),
        amount_0,
        amount_1,
    });

    pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.unlocked = true;
    Ok(())
}
