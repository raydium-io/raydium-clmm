use crate::states::*;
use crate::util::get_recent_epoch;
use anchor_lang::prelude::*;
use anchor_spl::token::spl_token::instruction::AuthorityType;
use anchor_spl::token::{set_authority, SetAuthority, Token};
use anchor_spl::token_interface::TokenAccount;

pub const LOCK_POSITION_SEED: &str = "locked_position";
#[derive(Accounts)]
pub struct LockPosition<'info> {
    /// The position owner or delegated authority
    #[account(mut)]
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        token::token_program = token_program,
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Decrease liquidity for this position
    #[account()]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    #[account(
        init,
        seeds = [
            LOCKED_POSITION_SEED.as_bytes(),
            personal_position.key().as_ref(),
        ],
        bump,
        payer = nft_owner,
        space = LockedPositionState::LEN
    )]
    pub locked_position: Box<Account<'info, LockedPositionState>>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,
}

pub fn lock_position<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, LockPosition<'info>>,
) -> Result<()> {
    require_gt!(ctx.accounts.personal_position.liquidity, 0);
    ctx.accounts.locked_position.initialize(
        ctx.bumps.locked_position,
        ctx.accounts.nft_owner.key(),
        ctx.accounts.personal_position.pool_id,
        ctx.accounts.personal_position.key(),
        ctx.accounts.nft_account.key(),
        get_recent_epoch()?,
    );

    let cpi_context = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        SetAuthority {
            current_authority: ctx.accounts.nft_owner.to_account_info(),
            account_or_mint: ctx.accounts.nft_account.to_account_info(),
        },
    );
    set_authority(cpi_context, AuthorityType::AccountOwner, Some(crate::id()))?;

    Ok(())
}
