use crate::error::ErrorCode;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

/// Checks whether the transaction time has not crossed the deadline
///
/// # Arguments
///
/// * `deadline` - The deadline specified by a user
///
pub fn check_deadline(deadline: i64) -> Result<()> {
    require!(
        Clock::get()?.unix_timestamp <= deadline,
        ErrorCode::TransactionTooOld
    );
    Ok(())
}

/// Ensures that the signer is the owner or a delgated authority for the position NFT
///
/// # Arguments
///
/// * `signer` - The signer address
/// * `token_account` - The token account holding the position NFT
///
pub fn is_authorized_for_token<'info>(
    signer: &Signer<'info>,
    token_account: &Box<Account<'info, TokenAccount>>,
) -> Result<()> {
    require!(
        token_account.amount == 1
            && (token_account.owner == signer.key()
                || (token_account.delegate.contains(&signer.key())
                    && token_account.delegated_amount > 0)),
        ErrorCode::NotApproved
    );
    Ok(())
}
