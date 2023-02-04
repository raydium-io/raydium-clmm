use super::calculate_latest_token_fees;
use super::modify_position;
use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use std::cell::RefMut;

#[derive(Accounts)]
pub struct DecreaseLiquidity<'info> {
    /// The position owner or delegated authority
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Decrease liquidity for this position
    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(
        mut,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &personal_position.tick_lower_index.to_be_bytes(),
            &personal_position.tick_upper_index.to_be_bytes(),
        ],
        bump,
        constraint = protocol_position.pool_id == pool_state.key(),
    )]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    /// Token_0 vault
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// Token_1 vault
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// Stores init state for the lower tick
    #[account(mut, constraint = tick_array_lower.load()?.pool_id == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    #[account(mut, constraint = tick_array_upper.load()?.pool_id == pool_state.key())]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    /// The destination token account for receive amount_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub recipient_token_account_0: Box<Account<'info, TokenAccount>>,

    /// The destination token account for receive amount_1
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub recipient_token_account_1: Box<Account<'info, TokenAccount>>,

    /// SPL program to transfer out tokens
    pub token_program: Program<'info, Token>,
}

pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    assert!(liquidity <= ctx.accounts.personal_position.liquidity);
    {
        let pool_state = ctx.accounts.pool_state.load()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity)
            && !pool_state.get_status_by_bit(PoolStatusBitIndex::CollectFee)
            && !pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward)
        {
            return err!(ErrorCode::NotApproved);
        }
    }

    let (decrease_amount_0, latest_fees_owed_0, decrease_amount_1, latest_fees_owed_1) =
        decrease_liquidity_and_update_position(
            &ctx.accounts.pool_state,
            &mut ctx.accounts.protocol_position,
            &mut ctx.accounts.personal_position,
            &ctx.accounts.tick_array_lower,
            &ctx.accounts.tick_array_upper,
            liquidity,
            amount_0_min,
            amount_1_min,
        )?;
    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_0,
        &ctx.accounts.recipient_token_account_0,
        &ctx.accounts.token_program,
        decrease_amount_0 + latest_fees_owed_0,
    )?;

    transfer_from_pool_vault_to_user(
        &ctx.accounts.pool_state,
        &ctx.accounts.token_vault_1,
        &ctx.accounts.recipient_token_account_1,
        &ctx.accounts.token_program,
        decrease_amount_1 + latest_fees_owed_1,
    )?;

    check_unclaimed_fees_and_vault(
        &ctx.accounts.pool_state,
        &mut ctx.accounts.token_vault_0,
        &mut ctx.accounts.token_vault_1,
    )?;

    let personal_position = &mut ctx.accounts.personal_position;
    let reward_amounts = collect_rewards(
        &ctx.accounts.pool_state,
        ctx.remaining_accounts,
        ctx.accounts.token_program.clone(),
        personal_position,
    )?;

    #[cfg(feature = "enable-log")]
    msg!(
        "decrease_amount_0:{}, fees_owed_0:{}, decrease_amount_1:{}, fees_owed_1:{}, reward_amounts:{:?}",
        decrease_amount_0,
        latest_fees_owed_0,
        decrease_amount_1,
        latest_fees_owed_1,
        reward_amounts
    );
    emit!(DecreaseLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        decrease_amount_0: decrease_amount_0,
        decrease_amount_1: decrease_amount_1,
        fee_amount_0: latest_fees_owed_0,
        fee_amount_1: latest_fees_owed_1,
        reward_amounts
    });

    Ok(())
}

pub fn decrease_liquidity_and_update_position<'a, 'b, 'c, 'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    protocol_position: &mut Box<Account<'info, ProtocolPositionState>>,
    personal_position: &mut Box<Account<'info, PersonalPositionState>>,
    tick_array_lower: &AccountLoader<'info, TickArrayState>,
    tick_array_upper: &AccountLoader<'info, TickArrayState>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<(u64, u64, u64, u64)> {
    let mut pool_state = pool_state_loader.load_mut()?;
    let mut decrease_amount_0 = 0;
    let mut decrease_amount_1 = 0;
    if pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity) {
        (decrease_amount_0, decrease_amount_1) = burn_liquidity(
            &mut pool_state,
            tick_array_lower,
            tick_array_upper,
            protocol_position,
            liquidity,
        )?;
        if liquidity > 0 {
            require!(
                decrease_amount_0 >= amount_0_min && decrease_amount_1 >= amount_1_min,
                ErrorCode::PriceSlippageCheck
            );
        }

        personal_position.token_fees_owed_0 = calculate_latest_token_fees(
            personal_position.token_fees_owed_0,
            personal_position.fee_growth_inside_0_last_x64,
            protocol_position.fee_growth_inside_0_last_x64,
            personal_position.liquidity,
        );

        personal_position.token_fees_owed_1 = calculate_latest_token_fees(
            personal_position.token_fees_owed_1,
            personal_position.fee_growth_inside_1_last_x64,
            protocol_position.fee_growth_inside_1_last_x64,
            personal_position.liquidity,
        );

        personal_position.fee_growth_inside_0_last_x64 =
            protocol_position.fee_growth_inside_0_last_x64;
        personal_position.fee_growth_inside_1_last_x64 =
            protocol_position.fee_growth_inside_1_last_x64;

        // update rewards, must update before decrease liquidity
        personal_position.update_rewards(protocol_position.reward_growth_inside, true)?;
        personal_position.liquidity = personal_position.liquidity.checked_sub(liquidity).unwrap();
    }

    let mut latest_fees_owed_0 = 0;
    let mut latest_fees_owed_1 = 0;
    if pool_state.get_status_by_bit(PoolStatusBitIndex::CollectFee) {
        latest_fees_owed_0 = personal_position.token_fees_owed_0;
        latest_fees_owed_1 = personal_position.token_fees_owed_1;

        require_gte!(
            pool_state.total_fees_token_0 - pool_state.total_fees_claimed_token_0,
            latest_fees_owed_0
        );
        require_gte!(
            pool_state.total_fees_token_1 - pool_state.total_fees_claimed_token_1,
            latest_fees_owed_1
        );

        personal_position.token_fees_owed_0 = 0;
        personal_position.token_fees_owed_1 = 0;

        pool_state.total_fees_claimed_token_0 = pool_state
            .total_fees_claimed_token_0
            .checked_add(latest_fees_owed_0)
            .unwrap();
        pool_state.total_fees_claimed_token_1 = pool_state
            .total_fees_claimed_token_1
            .checked_add(latest_fees_owed_1)
            .unwrap();
    }

    Ok((
        decrease_amount_0,
        latest_fees_owed_0,
        decrease_amount_1,
        latest_fees_owed_1,
    ))
}

pub fn burn_liquidity<'b, 'info>(
    pool_state: &mut RefMut<PoolState>,
    tick_array_lower_loader: &AccountLoader<'info, TickArrayState>,
    tick_array_upper_loader: &AccountLoader<'info, TickArrayState>,
    protocol_position: &mut ProtocolPositionState,
    liquidity: u128,
) -> Result<(u64, u64)> {
    require_keys_eq!(tick_array_lower_loader.load()?.pool_id, pool_state.key());
    require_keys_eq!(tick_array_upper_loader.load()?.pool_id, pool_state.key());
    let liquidity_before = pool_state.liquidity;
    // get tick_state
    let mut tick_lower_state = *tick_array_lower_loader.load_mut()?.get_tick_state_mut(
        protocol_position.tick_lower_index,
        i32::from(pool_state.tick_spacing),
    )?;
    let mut tick_upper_state = *tick_array_upper_loader.load_mut()?.get_tick_state_mut(
        protocol_position.tick_upper_index,
        i32::from(pool_state.tick_spacing),
    )?;
    let clock = Clock::get()?;
    let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
        -i128::try_from(liquidity).unwrap(),
        pool_state,
        protocol_position,
        &mut tick_lower_state,
        &mut tick_upper_state,
        clock.unix_timestamp as u64,
    )?;

    // update tick_state
    tick_array_lower_loader.load_mut()?.update_tick_state(
        protocol_position.tick_lower_index,
        i32::from(pool_state.tick_spacing),
        tick_lower_state,
    )?;
    tick_array_upper_loader.load_mut()?.update_tick_state(
        protocol_position.tick_upper_index,
        i32::from(pool_state.tick_spacing),
        tick_upper_state,
    )?;

    if flip_tick_lower {
        let mut tick_array_lower = tick_array_lower_loader.load_mut()?;
        tick_array_lower.update_initialized_tick_count(false)?;
        if tick_array_lower.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_lower.start_tick_index)?;
        }
    }
    if flip_tick_upper {
        let mut tick_array_upper = tick_array_upper_loader.load_mut()?;
        tick_array_upper.update_initialized_tick_count(false)?;
        if tick_array_upper.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_upper.start_tick_index)?;
        }
    }

    emit!(LiquidityChangeEvent {
        pool_state: pool_state.key(),
        tick: pool_state.tick_current,
        tick_lower: protocol_position.tick_lower_index,
        tick_upper: protocol_position.tick_upper_index,
        liquidity_before: liquidity_before,
        liquidity_after: pool_state.liquidity,
    });

    let amount_0 = u64::try_from(-amount_0_int).unwrap();
    let amount_1 = u64::try_from(-amount_1_int).unwrap();
    Ok((amount_0, amount_1))
}

pub fn collect_rewards<'a, 'b, 'c, 'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    remaining_accounts: &[AccountInfo<'info>],
    token_program: Program<'info, Token>,
    personal_position_state: &mut PersonalPositionState,
) -> Result<[u64; REWARD_NUM]> {
    let mut reward_amounts: [u64; REWARD_NUM] = [0, 0, 0];
    if !pool_state_loader
        .load()?
        .get_status_by_bit(PoolStatusBitIndex::CollectReward)
    {
        return Ok(reward_amounts);
    }
    check_required_accounts_length(pool_state_loader, remaining_accounts)?;

    let remaining_accounts_len = remaining_accounts.len();
    let mut remaining_accounts = remaining_accounts.iter();
    for i in 0..remaining_accounts_len / 2 {
        let reward_token_vault =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let recipient_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        require_keys_eq!(reward_token_vault.mint, recipient_token_account.mint);
        require_keys_eq!(
            reward_token_vault.key(),
            pool_state_loader.load_mut()?.reward_infos[i].token_vault
        );

        let reward_amount_owed = personal_position_state.reward_infos[i].reward_amount_owed;
        if reward_amount_owed == 0 {
            continue;
        }
        pool_state_loader
            .load()?
            .check_unclaimed_reward(i, reward_amount_owed)?;

        let transfer_amount = if reward_amount_owed > reward_token_vault.amount {
            reward_token_vault.amount
        } else {
            reward_amount_owed
        };

        if transfer_amount > 0 {
            msg!(
                "collect reward index: {}, transfer_amount: {}, reward_amount_owed:{} ",
                i,
                transfer_amount,
                reward_amount_owed
            );
            personal_position_state.reward_infos[i].reward_amount_owed =
                reward_amount_owed.checked_sub(transfer_amount).unwrap();
            pool_state_loader
                .load_mut()?
                .add_reward_clamed(i, transfer_amount)?;

            transfer_from_pool_vault_to_user(
                &pool_state_loader,
                &reward_token_vault,
                &recipient_token_account,
                &token_program,
                transfer_amount,
            )?;
        }
        reward_amounts[i] = transfer_amount
    }

    Ok(reward_amounts)
}

fn check_required_accounts_length(
    pool_state_loader: &AccountLoader<PoolState>,
    remaining_accounts: &[AccountInfo],
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    let mut valid_reward_count = 0;
    for item in pool_state.reward_infos {
        if item.initialized() {
            valid_reward_count = valid_reward_count + 1;
        }
    }
    let remaining_accounts_len = remaining_accounts.len();
    if remaining_accounts_len != valid_reward_count * 2 {
        return err!(ErrorCode::InvalidRewardInputAccountNumber);
    }
    Ok(())
}

pub fn check_unclaimed_fees_and_vault(
    pool_state_loader: &AccountLoader<PoolState>,
    token_vault_0: &mut Account<TokenAccount>,
    token_vault_1: &mut Account<TokenAccount>,
) -> Result<()> {
    token_vault_0.reload()?;
    token_vault_1.reload()?;

    let pool_state = &mut pool_state_loader.load_mut()?;

    let unclaimed_fee_token_0 = pool_state
        .total_fees_token_0
        .checked_sub(pool_state.total_fees_claimed_token_0)
        .unwrap();
    let unclaimed_fee_token_1 = pool_state
        .total_fees_token_1
        .checked_sub(pool_state.total_fees_claimed_token_1)
        .unwrap();

    if (unclaimed_fee_token_0 >= token_vault_0.amount && token_vault_0.amount != 0)
        || (unclaimed_fee_token_1 >= token_vault_1.amount && token_vault_1.amount != 0)
    {
        pool_state.set_status_by_bit(PoolStatusBitIndex::CollectFee, PoolStatusBitFlag::Disable);
    }
    Ok(())
}
