use crate::error::ErrorCode;
use crate::states::*;
use crate::util::{burn, close_account, close_spl_account};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// The position nft owner
    #[account(mut)]
    pub nft_owner: Signer<'info>,

    /// Unique token mint address
    #[account(
      mut,
      address = personal_position.nft_mint
    )]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    /// Token account where position NFT will be minted
    #[account(
        mut,
        associated_token::mint = position_nft_mint,
        associated_token::authority = nft_owner,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    // #[account(mut)]
    // pub metadata_account: UncheckedAccount<'info>,

    /// Metadata for the tokenized position
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,
    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
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
    if ctx.accounts.position_nft_account.amount == 1 {
        burn(
            &ctx.accounts.nft_owner,
            &ctx.accounts.position_nft_mint,
            &ctx.accounts.position_nft_account,
            &ctx.accounts.token_program,
            &[],
            1,
        )?;
    }

    close_spl_account(
        &ctx.accounts.nft_owner,
        &ctx.accounts.nft_owner.to_account_info(),
        &ctx.accounts.position_nft_account.to_account_info(),
        &ctx.accounts.token_program,
        &[],
    )?;

    close_account(
        &ctx.accounts.personal_position.to_account_info(),
        &ctx.accounts.nft_owner.to_account_info(),
    )?;

    // close_spl_account(
    //     &ctx.accounts.amm_config.to_account_info(),
    //     &ctx.accounts.nft_owner.to_account_info(),
    //     &ctx.accounts.position_nft_mint.to_account_info(),
    //     &ctx.accounts.token_program,
    //     &[&[
    //         AMM_CONFIG_SEED.as_bytes(),
    //         &[ctx.accounts.amm_config.bump] as &[u8],
    //     ]],
    // )?;

    Ok(())
}
