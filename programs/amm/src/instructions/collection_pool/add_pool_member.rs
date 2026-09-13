use crate::error::ErrorCode;
use crate::states::*;
use crate::util::create_token_vault_account;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

/// Adds a collection member to a collection pool: creates its vault owned by the pool state.
/// Permissionless; the mint must already be a member of the pool's collection.
#[derive(Accounts)]
pub struct AddPoolMember<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump = pool_members.bump,
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    #[account(
        seeds = [
            COLLECTION_MEMBER_SEED.as_bytes(),
            pool_members.collection.as_ref(),
            mint.key().as_ref()
        ],
        bump = collection_member.bump,
        constraint = collection_member.collection == pool_members.collection @ ErrorCode::InvalidCollectionMember,
    )]
    pub collection_member: Box<Account<'info, CollectionMember>>,

    #[account(mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: created here
    #[account(
        mut,
        seeds = [MEMBER_VAULT_SEED.as_bytes(), pool_state.key().as_ref(), mint.key().as_ref()],
        bump,
    )]
    pub member_vault: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn add_pool_member(ctx: Context<AddPoolMember>) -> Result<()> {
    let pm = &mut ctx.accounts.pool_members;
    let n = pm.n as usize;
    require!(n < MAX_POOL_MEMBERS, ErrorCode::TooManyPoolMembers);
    let mint = ctx.accounts.mint.key();
    let pool_key = ctx.accounts.pool_state.key();
    require!(pm.find(&mint).is_none(), ErrorCode::PoolMemberExists);
    {
        let pool = ctx.accounts.pool_state.load()?;
        require!(mint != pool.token_mint_0 && mint != pool.token_mint_1, ErrorCode::PoolMemberExists);
    }
    require_eq!(ctx.accounts.mint.decimals, pm.members[0].decimals, ErrorCode::MemberDecimalsMismatch);
    create_token_vault_account(
        &ctx.accounts.payer,
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.member_vault.to_account_info(),
        &ctx.accounts.mint,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &[MEMBER_VAULT_SEED.as_bytes(), pool_key.as_ref(), mint.as_ref(), &[ctx.bumps.member_vault][..]],
    )?;
    pm.members[n] = PoolMember { mint, vault: ctx.accounts.member_vault.key(), decimals: ctx.accounts.mint.decimals, ..Default::default() };
    pm.n += 1;
    Ok(())
}
