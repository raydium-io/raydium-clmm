use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>,
    from: &Account<'info, TokenAccount>,
    to_vault: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    msg!("deposit to vault amount {}", amount);
    token::transfer(
        CpiContext::new(
            token_program.to_account_info(),
            Transfer {
                from: from.to_account_info(),
                to: to_vault.to_account_info(),
                authority: signer.to_account_info(),
            },
        ),
        amount,
    )
}

pub fn transfer_from_pool_vault_to_user<'info>(
    pool: &mut Account<'info, PoolState>,
    from_vault: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    msg!("withdraw from vault amount {}", amount);
    let pool_state_seeds = [
        &POOL_SEED.as_bytes(),
        &pool.token_mint_0.to_bytes() as &[u8],
        &pool.token_mint_1.to_bytes() as &[u8],
        &pool.fee.to_be_bytes(),
        &[pool.bump],
    ];
    token::transfer(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            Transfer {
                from: from_vault.to_account_info(),
                to: to.to_account_info(),
                authority: pool.to_account_info(),
            },
            &[&pool_state_seeds[..]],
        ),
        amount,
    )
}
