use crate::error::ErrorCode;
use crate::libraries::stable;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

const FEE_DENOM: u128 = FEE_RATE_DENOMINATOR_VALUE as u128;

/// Swap between two members of a collection pool, inside the pool. Priced by a StableSwap over all
/// members' rate-normalised reserves (exactly the collection rate at balance); fee is
/// `amm_config.trade_fee_rate / collection.rebalance_fee_divisor` on input, split like any swap.
#[derive(Accounts)]
pub struct IntraSwap<'info> {
    pub payer: Signer<'info>,

    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [POOL_MEMBERS_SEED.as_bytes(), pool_state.key().as_ref()],
        bump = pool_members.bump,
    )]
    pub pool_members: Box<Account<'info, PoolMembers>>,

    #[account(address = pool_members.collection @ ErrorCode::InvalidCollectionMember)]
    pub collection: Box<Account<'info, TokenCollection>>,

    #[account(mut, token::mint = input_vault.mint, token::authority = payer)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = output_vault.mint)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    #[account(address = input_vault.mint)]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(address = output_vault.mint)]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,
    // remaining accounts, for k in 0..pool_members.n: [member vault k, CollectionMember k]
}

#[event]
pub struct IntraSwapEvent {
    pub pool_state: Pubkey,
    pub input_mint: Pubkey,
    pub output_mint: Pubkey,
    pub input_amount: u64,
    pub output_amount: u64,
    pub trade_fee: u64,
}

fn ceil_div(a: u128, num: u128, den: u128) -> Option<u128> {
    a.checked_mul(num)?.checked_add(den - 1)?.checked_div(den)
}

pub fn intra_swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, IntraSwap<'info>>,
    from_index: u8,
    to_index: u8,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    let block_timestamp = Clock::get()?.unix_timestamp as u64;
    let pm = &ctx.accounts.pool_members;
    let n = pm.n as usize;
    let (i, j) = (from_index as usize, to_index as usize);
    require!(i < n && j < n && i != j, ErrorCode::InvalidPoolMember);
    require_keys_eq!(ctx.accounts.input_vault.key(), pm.members[i].vault, ErrorCode::InvalidPoolMember);
    require_keys_eq!(ctx.accounts.output_vault.key(), pm.members[j].vault, ErrorCode::InvalidPoolMember);
    require!(ctx.remaining_accounts.len() >= 2 * n, ErrorCode::InvalidPoolMember);
    let pool_key = ctx.accounts.pool_state.key();
    let pool = ctx.accounts.pool_state.load()?;
    require!(pool.get_status_by_bit(PoolStatusBitIndex::Swap) && block_timestamp > pool.open_time, ErrorCode::NotApproved);

    let mut reserves = [0u128; MAX_POOL_MEMBERS];
    let mut rates = [0u64; MAX_POOL_MEMBERS];
    for k in 0..n {
        let vault_info = &ctx.remaining_accounts[2 * k];
        let member_info = &ctx.remaining_accounts[2 * k + 1];
        require_keys_eq!(vault_info.key(), pm.members[k].vault, ErrorCode::InvalidPoolMember);
        let vault = InterfaceAccount::<TokenAccount>::try_from(vault_info)?;
        let member = Account::<CollectionMember>::try_from(member_info)?;
        require!(member.collection == pm.collection && member.mint == pm.members[k].mint, ErrorCode::InvalidCollectionMember);
        let owed = if k == 0 {
            if pm.base_is_token_0 { pool.protocol_fees_token_0 + pool.fund_fees_token_0 } else { pool.protocol_fees_token_1 + pool.fund_fees_token_1 }
        } else {
            pm.members[k].protocol_fees_owed + pm.members[k].fund_fees_owed
        };
        reserves[k] = u128::from(vault.amount.checked_sub(owed).ok_or(ErrorCode::CalculateOverflow)?);
        rates[k] = member.rate;
    }
    drop(pool);
    let mut xp = vec![0u128; n];
    for k in 0..n {
        xp[k] = reserves[k].checked_mul(u128::from(rates[k])).ok_or(ErrorCode::CalculateOverflow)?;
    }

    let transfer_fee = get_transfer_fee(ctx.accounts.input_vault_mint.clone(), amount_in)?;
    let actual_in = amount_in.saturating_sub(transfer_fee);
    require_gt!(actual_in, 0, ErrorCode::TooSmallInputOrOutputAmount);
    // Never below 1 ppm.
    let fee_rate = u128::from((ctx.accounts.amm_config.trade_fee_rate / ctx.accounts.collection.rebalance_fee_divisor).max(1));
    let trade_fee = ceil_div(u128::from(actual_in), fee_rate, FEE_DENOM).ok_or(ErrorCode::CalculateOverflow)?;
    let protocol_fee = trade_fee * u128::from(ctx.accounts.amm_config.protocol_fee_rate) / FEE_DENOM;
    let fund_fee = trade_fee * u128::from(ctx.accounts.amm_config.fund_fee_rate) / FEE_DENOM;
    let net_in = u128::from(actual_in).checked_sub(trade_fee).ok_or(ErrorCode::CalculateOverflow)?;
    let dx = net_in.checked_mul(u128::from(rates[i])).ok_or(ErrorCode::CalculateOverflow)?;
    let dy = stable::swap_out(&xp, i, j, dx, pm.amp).ok_or(ErrorCode::StableCurveConvergence)?;
    let amount_out = u64::try_from(dy / u128::from(rates[j])).map_err(|_| ErrorCode::CalculateOverflow)?;
    require_gt!(amount_out, 0, ErrorCode::TooSmallInputOrOutputAmount);
    require!(u128::from(amount_out) < reserves[j], ErrorCode::InsufficientMemberLiquidity);
    let out_transfer_fee = get_transfer_fee(ctx.accounts.output_vault_mint.clone(), amount_out)?;
    require_gte!(amount_out.checked_sub(out_transfer_fee).ok_or(ErrorCode::CalculateOverflow)?, minimum_amount_out, ErrorCode::TooLittleOutputReceived);

    let (protocol_fee, fund_fee) = (u64::try_from(protocol_fee).unwrap(), u64::try_from(fund_fee).unwrap());
    if i == 0 {
        let mut pool = ctx.accounts.pool_state.load_mut()?;
        if pm.base_is_token_0 {
            pool.protocol_fees_token_0 = pool.protocol_fees_token_0.checked_add(protocol_fee).unwrap();
            pool.fund_fees_token_0 = pool.fund_fees_token_0.checked_add(fund_fee).unwrap();
        } else {
            pool.protocol_fees_token_1 = pool.protocol_fees_token_1.checked_add(protocol_fee).unwrap();
            pool.fund_fees_token_1 = pool.fund_fees_token_1.checked_add(fund_fee).unwrap();
        }
    } else {
        let pm = &mut ctx.accounts.pool_members;
        pm.members[i].protocol_fees_owed = pm.members[i].protocol_fees_owed.checked_add(protocol_fee).unwrap();
        pm.members[i].fund_fees_owed = pm.members[i].fund_fees_owed.checked_add(fund_fee).unwrap();
    }

    transfer_from_user_to_pool_vault(
        &ctx.accounts.payer,
        &ctx.accounts.input_token_account.to_account_info(),
        &ctx.accounts.input_vault.to_account_info(),
        Some(ctx.accounts.input_vault_mint.clone()),
        &ctx.accounts.token_program.to_account_info(),
        Some(ctx.accounts.token_program_2022.to_account_info()),
        amount_in,
    )?;
    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.output_vault.to_account_info(),
        &ctx.accounts.output_token_account.to_account_info(),
        Some(ctx.accounts.output_vault_mint.clone()),
        &ctx.accounts.token_program.to_account_info(),
        Some(ctx.accounts.token_program_2022.to_account_info()),
        amount_out,
    )?;
    emit!(IntraSwapEvent {
        pool_state: pool_key,
        input_mint: ctx.accounts.input_vault_mint.key(),
        output_mint: ctx.accounts.output_vault_mint.key(),
        input_amount: actual_in,
        output_amount: amount_out,
        trade_fee: u64::try_from(trade_fee).unwrap(),
    });
    Ok(())
}
