use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateTokenCollection<'info> {
    /// Anyone may create a collection
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Admin-defined ruleset members must satisfy
    pub ruleset: Account<'info, Ruleset>,

    /// Collection numeraire; admitted without a rule check
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        seeds = [TOKEN_COLLECTION_SEED.as_bytes(), authority.key().as_ref(), &index.to_le_bytes()],
        bump,
        payer = authority,
        space = TokenCollection::LEN
    )]
    pub collection: Account<'info, TokenCollection>,

    pub system_program: Program<'info, System>,
}

pub fn create_token_collection(
    ctx: Context<CreateTokenCollection>,
    index: u16,
    rebalance_fee_divisor: u32,
) -> Result<()> {
    require_gt!(rebalance_fee_divisor, 0, ErrorCode::InvalidUpdateConfigFlag);
    let collection = &mut ctx.accounts.collection;
    collection.bump = ctx.bumps.collection;
    collection.index = index;
    collection.authority = ctx.accounts.authority.key();
    collection.ruleset = ctx.accounts.ruleset.key();
    collection.quote_mint = ctx.accounts.quote_mint.key();
    collection.anchor_mint = ctx.remaining_accounts.first().map(|a| a.key()).unwrap_or_default();
    collection.rebalance_fee_divisor = rebalance_fee_divisor;
    Ok(())
}
