use crate::error::ErrorCode;
use crate::states::*;
use crate::util::{burn, close_spl_account};
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// The position nft owner
    #[account(mut)]
    pub nft_owner: Signer<'info>,

    /// Mint address bound to the personal position.
    #[account(
      mut,
      address = personal_position.nft_mint,
      mint::token_program = token_program,
    )]
    pub position_nft_mint: Box<InterfaceAccount<'info, Mint>>,

    /// User token account where position NFT be minted to
    #[account(
        mut,
        token::mint = position_nft_mint,
        token::authority = nft_owner,
        constraint = position_nft_account.amount == 1,
        token::token_program = token_program,
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut, 
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        close = nft_owner
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// System program to close the position state account
    pub system_program: Program<'info, System>,

    /// Token/Token2022 program to close token/mint account
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn close_position<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ClosePosition<'info>>,
) -> Result<()> {
    if ctx.accounts.personal_position.liquidity != 0
        || ctx.accounts.personal_position.token_fees_owed_0 != 0
        || ctx.accounts.personal_position.token_fees_owed_1 != 0
    {
        msg!(
            "remaing liquidity:{},token_fees_owed_0:{},token_fees_owed_1:{}",
            ctx.accounts.personal_position.liquidity,
            ctx.accounts.personal_position.token_fees_owed_0,
            ctx.accounts.personal_position.token_fees_owed_1
        );
        return err!(ErrorCode::ClosePositionErr);
    }

    for i in 0..ctx.accounts.personal_position.reward_infos.len() {
        if ctx.accounts.personal_position.reward_infos[i].reward_amount_owed != 0 {
            msg!(
                "remaing reward index:{},amount:{}",
                i,
                ctx.accounts.personal_position.reward_infos[i].reward_amount_owed,
            );
            return err!(ErrorCode::ClosePositionErr);
        }
    }

    let token_program = ctx.accounts.token_program.to_account_info();
    let position_nft_mint = ctx.accounts.position_nft_mint.to_account_info();
    let personal_nft_account = ctx.accounts.position_nft_account.to_account_info();
    burn(
        &ctx.accounts.nft_owner,
        &position_nft_mint,
        &personal_nft_account,
        &token_program,
        &[],
        1,
    )?;

    // close use nft token account
    close_spl_account(
        &ctx.accounts.nft_owner,
        &ctx.accounts.nft_owner,
        &personal_nft_account,
        &token_program,
        &[],
    )?;

    if *position_nft_mint.owner == spl_token_2022::id() {
        // close nft mint account
        close_spl_account(
            &ctx.accounts.personal_position.to_account_info(),
            &ctx.accounts.nft_owner,
            &position_nft_mint,
            &token_program,
            &[&ctx.accounts.personal_position.seeds()],
        )?;
    }
    Ok(())
}
