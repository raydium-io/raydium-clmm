use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

/// Binds an existing CLMM pair to a token collection, turning it into a collection pool. The pair
/// must contain the collection's quote mint; the other token becomes member 0 (the base).
#[derive(Accounts)]
pub struct InitPoolMembers<'info> {
    /// Pool owner or admin
    #[account(
        mut,
        constraint = (payer.key() == pool_state.load()?.owner || payer.key() == crate::admin::ID) @ ErrorCode::NotApproved
    )]
    pub payer: Signer<'info>,

    pub pool_state: AccountLoader<'info, PoolState>,

    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            collection.key().as_ref(),
            base_member.mint.as_ref()
        ],
        bump = base_member.bump,
        constraint = base_member.collection == collection.key() @ ErrorCode::InvalidCollectionMember,
    )]
    pub base_member: Box<Account<'info, CollectionMember>>,

    #[account(
        init,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump,
        payer = payer,
        space = PoolMembers::LEN
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    pub system_program: Program<'info, System>,
}

pub fn init_pool_members(ctx: Context<InitPoolMembers>, amp: u64) -> Result<()> {
    require!((1..=1_000_000).contains(&amp), ErrorCode::InvalidAmp);
    let pool = ctx.accounts.pool_state.load()?;
    let quote = ctx.accounts.collection.quote_mint;
    let (base_is_token_0, base_mint, base_vault, decimals) = if pool.token_mint_1 == quote {
        (true, pool.token_mint_0, pool.token_vault_0, pool.mint_decimals_0)
    } else if pool.token_mint_0 == quote {
        (false, pool.token_mint_1, pool.token_vault_1, pool.mint_decimals_1)
    } else {
        return err!(ErrorCode::PoolNotInCollection);
    };
    require_keys_eq!(ctx.accounts.base_member.mint, base_mint, ErrorCode::InvalidCollectionMember);
    let pm = &mut ctx.accounts.pool_members;
    pm.bump = ctx.bumps.pool_members;
    pm.base_is_token_0 = base_is_token_0;
    pm.n = 1;
    pm.pool = ctx.accounts.pool_state.key();
    pm.collection = ctx.accounts.collection.key();
    pm.amp = amp;
    pm.members[0] = PoolMember { mint: base_mint, vault: base_vault, decimals, ..Default::default() };
    Ok(())
}
