use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

/// Re-reads an LST member's exchange rate from its stake pool. Permissionless, no signer needed.
#[derive(Accounts)]
pub struct SyncMemberRate<'info> {
    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(address = collection.ruleset @ ErrorCode::InvalidUpdateConfigFlag)]
    pub ruleset: Box<Account<'info, Ruleset>>,

    #[account(mut, constraint = member.collection == collection.key() @ ErrorCode::InvalidCollectionMember)]
    pub member: Box<Account<'info, CollectionMember>>,

    /// CHECK: validated against the ruleset's program and the member mint in the handler
    pub stake_pool: UncheckedAccount<'info>,
}

pub fn sync_member_rate(ctx: Context<SyncMemberRate>) -> Result<()> {
    require!(RuleKind::from_u8(ctx.accounts.ruleset.kind) == Some(RuleKind::Lst), ErrorCode::InvalidRuleKind);
    require_keys_neq!(ctx.accounts.member.mint, ctx.accounts.collection.quote_mint, ErrorCode::InvalidCollectionMember);
    let rate = super::stake_pool_rate(&ctx.accounts.ruleset, &ctx.accounts.member.mint, &ctx.accounts.stake_pool.to_account_info())?;
    ctx.accounts.member.rate = rate;
    Ok(())
}
