use std::cell::RefMut;

use super::calculate_latest_token_fees;
use super::modify_position;
use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

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
    #[account(mut, constraint = tick_array_lower.load()?.amm_pool == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    #[account(mut, constraint = tick_array_upper.load()?.amm_pool == pool_state.key())]
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
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let protocol_position_state = &mut ctx.accounts.protocol_position;

    let (decrease_amount_0, decrease_amount_1) = burn_liquidity(
        &mut pool_state,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        protocol_position_state,
        liquidity,
    )?;
    if liquidity > 0 {
        require!(
            decrease_amount_0 >= amount_0_min && decrease_amount_1 >= amount_1_min,
            ErrorCode::PriceSlippageCheck
        );
    }

    let personal_position = &mut ctx.accounts.personal_position;
    personal_position.token_fees_owed_0 = calculate_latest_token_fees(
        personal_position.token_fees_owed_0,
        personal_position.fee_growth_inside_0_last_x64,
        protocol_position_state.fee_growth_inside_0_last_x64,
        personal_position.liquidity,
    );

    personal_position.token_fees_owed_1 = calculate_latest_token_fees(
        personal_position.token_fees_owed_1,
        personal_position.fee_growth_inside_1_last_x64,
        protocol_position_state.fee_growth_inside_1_last_x64,
        personal_position.liquidity,
    );

    personal_position.fee_growth_inside_0_last_x64 =
        protocol_position_state.fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last_x64 =
        protocol_position_state.fee_growth_inside_1_last_x64;

    let latest_fees_owed_0 = personal_position.token_fees_owed_0;
    let latest_fees_owed_1 = personal_position.token_fees_owed_1;
    personal_position.token_fees_owed_0 = 0;
    personal_position.token_fees_owed_1 = 0;

    // update rewards, must update before decrease liquidity
    personal_position.update_rewards(protocol_position_state.reward_growth_inside)?;
    personal_position.liquidity = personal_position.liquidity.checked_sub(liquidity).unwrap();

    let transfer_amount_0 = decrease_amount_0 + latest_fees_owed_0;
    let transfer_amount_1 = decrease_amount_1 + latest_fees_owed_1;

    if transfer_amount_0 > 0 {
        msg!(
            "decrease amount:{}, fee amount:{}",
            decrease_amount_0,
            latest_fees_owed_0,
        );
        transfer_from_pool_vault_to_user(
            &pool_state,
            &ctx.accounts.pool_state,
            &ctx.accounts.token_vault_0,
            &ctx.accounts.recipient_token_account_0,
            &ctx.accounts.token_program,
            transfer_amount_0,
        )?;
    }
    if transfer_amount_1 > 0 {
        msg!(
            "decrease amount:{}, fee amount:{}",
            decrease_amount_1,
            latest_fees_owed_1,
        );
        transfer_from_pool_vault_to_user(
            &pool_state,
            &ctx.accounts.pool_state,
            &ctx.accounts.token_vault_1,
            &ctx.accounts.recipient_token_account_1,
            &ctx.accounts.token_program,
            transfer_amount_1,
        )?;
    }

    let reward_amounts = collect_rewards(
        &mut pool_state,
        &ctx.accounts.pool_state,
        ctx.remaining_accounts,
        ctx.accounts.token_program.clone(),
        personal_position,
    )?;

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

pub fn burn_liquidity<'b, 'info>(
    pool_state: &mut RefMut<PoolState>,
    tick_array_lower_state: &AccountLoader<'info, TickArrayState>,
    tick_array_upper_state: &AccountLoader<'info, TickArrayState>,
    protocol_position_state: &mut Box<Account<'info, ProtocolPositionState>>,
    liquidity: u128,
) -> Result<(u64, u64)> {
    let mut tick_array_lower = tick_array_lower_state.load_mut()?;
    let tick_lower_state = tick_array_lower.get_tick_state_mut(
        protocol_position_state.tick_lower_index,
        pool_state.tick_spacing as i32,
    )?;

    let mut tick_array_upper = tick_array_upper_state.load_mut()?;
    let tick_upper_state = tick_array_upper.get_tick_state_mut(
        protocol_position_state.tick_upper_index,
        pool_state.tick_spacing as i32,
    )?;
    let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
        -i128::try_from(liquidity).unwrap(),
        pool_state,
        protocol_position_state,
        tick_lower_state,
        tick_upper_state,
    )?;
    if flip_tick_lower {
        tick_array_lower.update_initialized_tick_count(false)?;
        if tick_array_lower.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_lower.start_tick_index)?;
        }
    }
    if flip_tick_upper {
        tick_array_upper.update_initialized_tick_count(false)?;
        if tick_array_upper.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_upper.start_tick_index)?;
        }
    }

    let amount_0 = (-amount_0_int) as u64;
    let amount_1 = (-amount_1_int) as u64;
    Ok((amount_0, amount_1))
}

pub fn collect_rewards<'a, 'b, 'c, 'info>(
    pool_state: &mut RefMut<PoolState>,
    authority: &AccountLoader<'info, PoolState>,
    remaining_accounts: &[AccountInfo<'info>],
    token_program: Program<'info, Token>,
    personal_position_state: &mut PersonalPositionState,
) -> Result<[u64; REWARD_NUM]> {
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
    let mut reward_amounts: [u64; REWARD_NUM] = [0, 0, 0];
    let mut remaining_accounts = remaining_accounts.iter();
    for i in 0..remaining_accounts_len / 2 {
        let reward_token_vault =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        let recipient_token_account =
            Account::<TokenAccount>::try_from(&remaining_accounts.next().unwrap())?;
        require_keys_eq!(reward_token_vault.mint, recipient_token_account.mint);
        require_keys_eq!(
            reward_token_vault.key(),
            pool_state.reward_infos[i].token_vault
        );

        let reward_amount_owed = personal_position_state.reward_infos[i].reward_amount_owed;
        if reward_amount_owed == 0 {
            continue;
        }

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

            transfer_from_pool_vault_to_user(
                &pool_state,
                &authority,
                &reward_token_vault,
                &recipient_token_account,
                &token_program,
                transfer_amount,
            )?;

            pool_state.add_reward_clamed(i, transfer_amount)?;
        }
        reward_amounts[i] = transfer_amount
    }

    Ok(reward_amounts)
}
