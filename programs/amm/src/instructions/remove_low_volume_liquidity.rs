use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct RemoveLowVolumeLiquidity<'info> {
    /// The pool state account
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The pool creator or admin who can invoke this instruction
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Token_0 vault
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// Token_1 vault
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

     /// The destination token account for receive amount_0
     #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub recipient_token_account_0: Box<Account<'info, TokenAccount>>,

    /// The destination token account for receive amount_1
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub recipient_token_account_1: Box<Account<'info, TokenAccount>>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn remove_low_volume_liquidity(ctx: Context<RemoveLowVolumeLiquidity>) -> Result<()> {
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;

    if self.remove_liquidity_timestamp > Clock::get()?.unix_timestamp {
        transfer_from_pool_vault_to_user(
            &ctx.accounts.pool_state,
            &ctx.accounts.token_vault_0.to_account_info(),
            &ctx.accounts.recipient_token_account_0.to_account_info(),
            None,
            token_program,
            None,
            transfer_amount_0,
        )?;

        transfer_from_pool_vault_to_user(
            pool_state_loader,
            &ctx.accounts.token_vault_1.to_account_info(),
            &ctx.accounts.recipient_token_account_1.to_account_info(),
            None,
            token_program,
            None,
            transfer_amount_1,
        )?;
    }

    // Emit an event for liquidity removal
    emit!(LiquidityRemovedEvent {
        pool_state: ctx.accounts.pool_state.key(),
        volume: 0, // Volume not tracked on-chain; set to 0 or could be removed if not needed
    });

    Ok(())
}