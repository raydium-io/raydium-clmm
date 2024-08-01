use crate::error::ErrorCode;
use crate::states::*;
use crate::util::{burn, close_spl_account};
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, TokenAccount};

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// The position nft owner
    #[account(mut)]
    pub nft_owner: Signer<'info>,

    /// Unique token mint address
    #[account(
      mut,
      address = personal_position.nft_mint,
      mint::token_program = token_program,
    )]
    pub position_nft_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token account where position NFT will be minted
    #[account(
        mut,
        associated_token::mint = position_nft_mint,
        associated_token::authority = nft_owner,
        constraint = position_nft_account.amount == 1,
        token::token_program = token_program,
    )]
    pub position_nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    // #[account(mut)]
    // pub metadata_account: UncheckedAccount<'info>,

    /// Metadata for the tokenized position
    #[account(
        mut, 
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        close = nft_owner
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,
    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    // /// Reserved for upgrade
    // pub token_program_2022: Program<'info, Token2022>,
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

    burn(
        &ctx.accounts.nft_owner,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.position_nft_account,
        &ctx.accounts.token_program,
        // &ctx.accounts.token_program_2022,
        &[],
        1,
    )?;

    close_spl_account(
        &ctx.accounts.nft_owner,
        &ctx.accounts.nft_owner,
        &ctx.accounts.position_nft_account,
        &ctx.accounts.token_program,
        // &ctx.accounts.token_program_2022,
        &[],
    )?;

    Ok(())
}
