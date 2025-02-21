use crate::states::{AmmConfig, LiquidityPosition};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct LockLiquidityForever<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub personal_position: Account<'info, PersonalPositionState>, // The position to lock
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        constraint = nft_account.amount == 1,
        token::authority = nft_owner,
    )]
    pub nft_account: Account<'info, TokenAccount>, // The Position NFT proving ownership
    pub system_program: Program<'info, System>,
}

pub fn lock_liquidity_forever(ctx: Context<LockLiquidityForever>, liquidity: u128, lock_time: i64) -> Result<()> {
    
    let position = &mut ctx.accounts.personal_position;

        // Ensure it's not already locked
        require!(!position.locked_forever, ErrorCode::AlreadyLocked);

        // Lock the liquidity
        position.locked_forever = true;

        emit!(LiquidityLockedForeverEvent {
            position_nft_mint: position.nft_mint,
        });

        Ok(())
}
