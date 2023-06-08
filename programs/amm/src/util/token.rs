use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022,
    token_interface::{Mint, Token2022, TokenAccount},
};

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>,
    from: &InterfaceAccount<'info, TokenAccount>,
    to_vault: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    token_program: &Program<'info, Token>,
    token_program_2022: &Program<'info, Token2022>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    if from.owner == token_program_2022.key() {
        token_program_info = token_program_2022.to_account_info()
    }
    token_2022::transfer_checked(
        CpiContext::new(
            token_program_info,
            token_2022::TransferChecked {
                from: from.to_account_info(),
                to: to_vault.to_account_info(),
                authority: signer.to_account_info(),
                mint: mint.to_account_info(),
            },
        ),
        amount,
        mint.decimals,
    )
}

pub fn transfer_from_pool_vault_to_user<'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    from_vault: &InterfaceAccount<'info, TokenAccount>,
    to: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    token_program: &Program<'info, Token>,
    token_program_2022: &Program<'info, Token2022>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    if from_vault.owner == token_program_2022.key() {
        token_program_info = token_program_2022.to_account_info()
    }
    token_2022::transfer_checked(
        CpiContext::new_with_signer(
            token_program_info,
            token_2022::TransferChecked {
                from: from_vault.to_account_info(),
                to: to.to_account_info(),
                authority: pool_state_loader.to_account_info(),
                mint: mint.to_account_info(),
            },
            &[&pool_state_loader.load()?.seeds()],
        ),
        amount,
        mint.decimals,
    )
}

pub fn close_spl_account<'a, 'b, 'c, 'info>(
    owner: &AccountInfo<'info>,
    destination: &AccountInfo<'info>,
    close_account:&InterfaceAccount<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    token_program_2022: &Program<'info, Token2022>,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    let mut token_program_info = token_program.to_account_info();
    if close_account.owner == token_program_2022.key() {
        token_program_info = token_program_2022.to_account_info()
    }

    token_2022::close_account(CpiContext::new_with_signer(
        token_program_info,
        token_2022::CloseAccount {
            account: close_account.to_account_info(),
            destination: destination.to_account_info(),
            authority: owner.to_account_info(),
        },
        signers_seeds,
    ))
}

pub fn burn<'a, 'b, 'c, 'info>(
    owner: &Signer<'info>,
    mint: &InterfaceAccount<'info, Mint>,
    burn_account: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Program<'info, Token>,
    token_program_2022: &Program<'info, Token2022>,
    signers_seeds: &[&[&[u8]]],
    amount: u64,
) -> Result<()> {
    let mint_info = mint.to_account_info();
    let mut token_program_info = token_program.to_account_info();
    if mint_info.owner == token_program_2022.key {
        token_program_info = token_program_2022.to_account_info()
    }
    token_2022::burn(
        CpiContext::new_with_signer(
            token_program_info,
            token_2022::Burn {
                mint: mint_info,
                from: burn_account.to_account_info(),
                authority: owner.to_account_info(),
            },
            signers_seeds,
        ),
        amount,
    )
}
