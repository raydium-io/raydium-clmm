use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, CloseAccount, Mint, Token, TokenAccount, Transfer};

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>,
    from: &Account<'info, TokenAccount>,
    to_vault: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
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
    pool_state_loader: &AccountLoader<'info, PoolState>,
    from_vault: &Account<'info, TokenAccount>,
    to: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token::transfer(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            Transfer {
                from: from_vault.to_account_info(),
                to: to.to_account_info(),
                authority: pool_state_loader.to_account_info(),
            },
            &[&pool_state_loader.load()?.seeds()],
        ),
        amount,
    )
}

pub fn close_spl_account<'a, 'b, 'c, 'info>(
    owner: &AccountInfo<'info>,
    destination: &AccountInfo<'info>,
    close_account: &AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    token::close_account(CpiContext::new_with_signer(
        token_program.to_account_info(),
        CloseAccount {
            account: close_account.to_account_info(),
            destination: destination.to_account_info(),
            authority: owner.to_account_info(),
        },
        signers_seeds,
    ))
}

pub fn burn<'a, 'b, 'c, 'info>(
    owner: &Signer<'info>,
    mint: &Account<'info, Mint>,
    burn_account: &Account<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    signers_seeds: &[&[&[u8]]],
    amount: u64,
) -> Result<()> {
    token::burn(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            Burn {
                mint: mint.to_account_info(),
                from: burn_account.to_account_info(),
                authority: owner.to_account_info(),
            },
            signers_seeds,
        ),
        amount,
    )
}
