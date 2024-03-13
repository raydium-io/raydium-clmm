use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use switchboard_solana::FunctionAccountData;
use std::str::FromStr;
#[derive(Accounts)]
pub struct ChickumDinners<'info> {
    
    pub winner_winner_chickum_dinner: AccountInfo<'info>,
    pub nft_winner_winner_chickum_dinner: AccountInfo<'info>,
    #[account(mut, 
        constraint = 
            Pubkey::from_str("BXHY1pQcaqkhBxdjqpBrrbtirXaCuRJdXLSdqnYtDgsw").unwrap() == switchboard_function.key() @ ErrorCode::NotApproved)]
    pub pool_state: AccountLoader<'info, PoolState>,
    #[account(
    constraint =
                switchboard_function.load()?.validate(
                &enclave_signer.to_account_info()
            )? @ ErrorCode::NotApproved,
    
    )]
    pub switchboard_function: AccountLoader<'info, FunctionAccountData>,
    pub enclave_signer: Signer<'info>,
}

pub fn chickum_dinners(
    ctx: Context<ChickumDinners>
)-> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.winner_winner_chickum_dinner = *ctx.accounts.winner_winner_chickum_dinner.key;
    pool_state.nft_winner_winner_chickum_dinner = *ctx.accounts.nft_winner_winner_chickum_dinner.key;
    Ok(())
}

