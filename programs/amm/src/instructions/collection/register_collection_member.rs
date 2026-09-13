use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

#[derive(Accounts)]
pub struct RegisterCollectionMember<'info> {
    /// Anyone may register a mint that satisfies the ruleset
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(mut)]
    pub collection: Account<'info, TokenCollection>,

    #[account(address = collection.ruleset @ ErrorCode::InvalidUpdateConfigFlag)]
    pub ruleset: Account<'info, Ruleset>,

    pub mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        seeds = [COLLECTION_MEMBER_SEED.as_bytes(), collection.key().as_ref(), mint.key().as_ref()],
        bump,
        payer = payer,
        space = CollectionMember::LEN
    )]
    pub member: Account<'info, CollectionMember>,

    pub system_program: Program<'info, System>,
    // remaining accounts: rule proof (PumpFunLaunch: the mint's bonding curve)
}

pub fn register_collection_member<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, RegisterCollectionMember<'info>>,
) -> Result<()> {
    let mint_key = ctx.accounts.mint.key();
    let exempt = mint_key == ctx.accounts.collection.quote_mint || mint_key == ctx.accounts.collection.anchor_mint;
    if !exempt {
        super::check_rules(&ctx.accounts.ruleset, &ctx.accounts.mint, ctx.remaining_accounts)?;
    }
    let member = &mut ctx.accounts.member;
    member.bump = ctx.bumps.member;
    member.collection = ctx.accounts.collection.key();
    member.mint = ctx.accounts.mint.key();
    member.rate = if !exempt && RuleKind::from_u8(ctx.accounts.ruleset.kind) == Some(RuleKind::Lst)
    {
        super::stake_pool_rate(&ctx.accounts.ruleset, &ctx.accounts.mint.key(), &ctx.remaining_accounts[0])?
    } else {
        DEFAULT_MEMBER_RATE
    };
    member.registered_by = ctx.accounts.payer.key();
    ctx.accounts.collection.member_count = ctx.accounts.collection.member_count.saturating_add(1);
    emit!(CollectionMemberRegistered {
        collection: member.collection,
        mint: member.mint,
        ruleset: ctx.accounts.ruleset.key(),
        registered_by: member.registered_by,
    });
    Ok(())
}

#[event]
pub struct CollectionMemberRegistered {
    pub collection: Pubkey,
    pub mint: Pubkey,
    pub ruleset: Pubkey,
    pub registered_by: Pubkey,
}
