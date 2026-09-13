use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetCollectionMemberRate<'info> {
    #[account(address = collection.authority @ ErrorCode::NotApproved)]
    pub authority: Signer<'info>,

    pub collection: Account<'info, TokenCollection>,

    #[account(mut, constraint = member.collection == collection.key() @ ErrorCode::InvalidCollectionMember)]
    pub member: Account<'info, CollectionMember>,
}

/// Marks a member's value in the collection numeraire (1e9 == 1.0), e.g. an LST's exchange rate
/// or 1/price for a stable leg of a SOL-denominated collection. Only affects rebalance gating.
pub fn set_collection_member_rate(ctx: Context<SetCollectionMemberRate>, rate: u64) -> Result<()> {
    require_gt!(rate, 0, ErrorCode::InvalidUpdateConfigFlag);
    ctx.accounts.member.rate = rate;
    Ok(())
}
