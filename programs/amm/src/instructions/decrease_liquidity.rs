use super::calculate_latest_token_fees;
use super::modify_position;
use crate::error::ErrorCode;
use crate::states::*;
use crate::util::{self, transfer_from_pool_vault_to_user};
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token_2022::spl_token_2022;
use anchor_spl::token_interface::{self, Mint, Token2022};
use std::cell::RefMut;
use std::ops::Deref;

/// Memo msg for decrease liquidity
pub const DECREASE_MEMO_MSG: &'static [u8] = b"raydium_decrease";
#[derive(Accounts)]
pub struct DecreaseLiquidity<'info> {
    /// The position owner or delegated authority
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        constraint = nft_account.amount == 1,
        token::authority = nft_owner
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
    // remaining account
    // #[account(
    //     seeds = [
    //         POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
    //         pool_state.key().as_ref(),
    //     ],
    //     bump
    // )]
    // pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,
}

pub fn decrease_liquidity_v1<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    decrease_liquidity(
        &ctx.accounts.pool_state,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &ctx.accounts.recipient_token_account_0.to_account_info(),
        &ctx.accounts.recipient_token_account_1.to_account_info(),
        &ctx.accounts.token_program,
        None,
        None,
        None,
        None,
        &ctx.remaining_accounts,
        liquidity,
        amount_0_min,
        amount_1_min,
    )
}

pub fn decrease_liquidity<'a, 'b, 'c: 'info, 'info>(
    pool_state_loader: &'b AccountLoader<'info, PoolState>,
    protocol_position: &'b mut Box<Account<'info, ProtocolPositionState>>,
    personal_position: &'b mut Box<Account<'info, PersonalPositionState>>,
    token_vault_0: &'b AccountInfo<'info>,
    token_vault_1: &'b AccountInfo<'info>,
    tick_array_lower_loader: &'b AccountLoader<'info, TickArrayState>,
    tick_array_upper_loader: &'b AccountLoader<'info, TickArrayState>,
    recipient_token_account_0: &'b AccountInfo<'info>,
    recipient_token_account_1: &'b AccountInfo<'info>,
    token_program: &'b Program<'info, Token>,
    token_program_2022: Option<Program<'info, Token2022>>,
    _memo_program: Option<UncheckedAccount<'info>>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    remaining_accounts: &'c [AccountInfo<'info>],
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    // if accounts.memo_program.is_some() {
    //     let memp_program = accounts.memo_program.as_ref().unwrap().to_account_info();
    //     invoke_memo_instruction(DECREASE_MEMO_MSG, memp_program)?;
    // }
    assert!(liquidity <= personal_position.liquidity);
    let liquidity_before;
    let pool_sqrt_price_x64;
    let pool_tick_current;
    let mut tickarray_bitmap_extension = None;

    let remaining_collect_accounts = &mut Vec::new();
    {
        let pool_state = pool_state_loader.load()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity)
            && !pool_state.get_status_by_bit(PoolStatusBitIndex::CollectFee)
            && !pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward)
        {
            return err!(ErrorCode::NotApproved);
        }
        liquidity_before = pool_state.liquidity;
        pool_sqrt_price_x64 = pool_state.sqrt_price_x64;
        pool_tick_current = pool_state.tick_current;

        let use_tickarray_bitmap_extension = pool_state.is_overflow_default_tickarray_bitmap(vec![
            tick_array_lower_loader.load()?.start_tick_index,
            tick_array_upper_loader.load()?.start_tick_index,
        ]);

        for account_info in remaining_accounts.into_iter() {
            if account_info
                .key()
                .eq(&TickArrayBitmapExtension::key(pool_state.key()))
            {
                tickarray_bitmap_extension = Some(account_info);
                continue;
            }
            remaining_collect_accounts.push(account_info);
        }
        if use_tickarray_bitmap_extension {
            require!(
                tickarray_bitmap_extension.is_some(),
                ErrorCode::MissingTickArrayBitmapExtensionAccount
            );
        }
    }

    let (decrease_amount_0, latest_fees_owed_0, decrease_amount_1, latest_fees_owed_1) =
        decrease_liquidity_and_update_position(
            pool_state_loader,
            protocol_position,
            personal_position,
            tick_array_lower_loader,
            tick_array_upper_loader,
            tickarray_bitmap_extension,
            liquidity,
        )?;

    let mut transfer_fee_0 = 0;
    let mut transfer_fee_1 = 0;
    if vault_0_mint.is_some() {
        transfer_fee_0 =
            util::get_transfer_fee(vault_0_mint.clone().unwrap(), decrease_amount_0).unwrap();
    }
    if vault_1_mint.is_some() {
        transfer_fee_1 =
            util::get_transfer_fee(vault_1_mint.clone().unwrap(), decrease_amount_1).unwrap();
    }
    emit!(LiquidityCalculateEvent {
        pool_liquidity: liquidity_before,
        pool_sqrt_price_x64: pool_sqrt_price_x64,
        pool_tick: pool_tick_current,
        calc_amount_0: decrease_amount_0,
        calc_amount_1: decrease_amount_1,
        trade_fee_owed_0: latest_fees_owed_0,
        trade_fee_owed_1: latest_fees_owed_1,
        transfer_fee_0,
        transfer_fee_1,
    });
    #[cfg(feature = "enable-log")]
    msg!(
        "decrease_amount_0: {}, transfer_fee_0: {}, latest_fees_owed_0: {}, decrease_amount_1: {}, transfer_fee_1: {}, latest_fees_owed_1: {}",
        decrease_amount_0,
        transfer_fee_0,
        latest_fees_owed_0,
        decrease_amount_1,
        transfer_fee_1,
        latest_fees_owed_1
    );
    if liquidity > 0 {
        require_gte!(
            decrease_amount_0 - transfer_fee_0,
            amount_0_min,
            ErrorCode::PriceSlippageCheck
        );
        require_gte!(
            decrease_amount_1 - transfer_fee_1,
            amount_1_min,
            ErrorCode::PriceSlippageCheck
        );
    }
    let transfer_amount_0 = decrease_amount_0 + latest_fees_owed_0;
    let transfer_amount_1 = decrease_amount_1 + latest_fees_owed_1;

    let mut token_2022_program_opt: Option<AccountInfo> = None;
    if token_program_2022.is_some() {
        token_2022_program_opt = Some(token_program_2022.clone().unwrap().to_account_info());
    }

    transfer_from_pool_vault_to_user(
        pool_state_loader,
        &token_vault_0.to_account_info(),
        recipient_token_account_0,
        vault_0_mint,
        token_program,
        token_2022_program_opt.clone(),
        transfer_amount_0,
    )?;

    transfer_from_pool_vault_to_user(
        pool_state_loader,
        &token_vault_1.to_account_info(),
        recipient_token_account_1,
        vault_1_mint.clone(),
        token_program,
        token_2022_program_opt.clone(),
        transfer_amount_1,
    )?;

    check_unclaimed_fees_and_vault(pool_state_loader, token_vault_0, token_vault_1)?;

    let reward_amounts = collect_rewards(
        pool_state_loader,
        remaining_collect_accounts.as_slice(),
        token_program,
        token_2022_program_opt.clone(),
        personal_position,
        if token_2022_program_opt.is_none() {
            false
        } else {
            true
        },
    )?;
    emit!(DecreaseLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        decrease_amount_0: decrease_amount_0,
        decrease_amount_1: decrease_amount_1,
        fee_amount_0: latest_fees_owed_0,
        fee_amount_1: latest_fees_owed_1,
        reward_amounts,
        transfer_fee_0: transfer_fee_0,
        transfer_fee_1: transfer_fee_1,
    });

    Ok(())
}

pub fn decrease_liquidity_and_update_position<'a, 'b, 'c: 'info, 'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    protocol_position: &mut Box<Account<'info, ProtocolPositionState>>,
    personal_position: &mut Box<Account<'info, PersonalPositionState>>,
    tick_array_lower: &AccountLoader<'info, TickArrayState>,
    tick_array_upper: &AccountLoader<'info, TickArrayState>,
    tick_array_bitmap_extension: Option<&'c AccountInfo<'info>>,
    liquidity: u128,
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
            tick_array_bitmap_extension,
            liquidity,
        )?;

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

pub fn burn_liquidity<'c: 'info, 'info>(
    pool_state: &mut RefMut<PoolState>,
    tick_array_lower_loader: &AccountLoader<'info, TickArrayState>,
    tick_array_upper_loader: &AccountLoader<'info, TickArrayState>,
    protocol_position: &mut ProtocolPositionState,
    tickarray_bitmap_extension: Option<&'c AccountInfo<'info>>,
    liquidity: u128,
) -> Result<(u64, u64)> {
    require_keys_eq!(tick_array_lower_loader.load()?.pool_id, pool_state.key());
    require_keys_eq!(tick_array_upper_loader.load()?.pool_id, pool_state.key());
    let liquidity_before = pool_state.liquidity;
    // get tick_state
    let mut tick_lower_state = *tick_array_lower_loader
        .load_mut()?
        .get_tick_state_mut(protocol_position.tick_lower_index, pool_state.tick_spacing)?;
    let mut tick_upper_state = *tick_array_upper_loader
        .load_mut()?
        .get_tick_state_mut(protocol_position.tick_upper_index, pool_state.tick_spacing)?;
    let clock = Clock::get()?;
    let (amount_0, amount_1, flip_tick_lower, flip_tick_upper) = modify_position(
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
        pool_state.tick_spacing,
        tick_lower_state,
    )?;
    tick_array_upper_loader.load_mut()?.update_tick_state(
        protocol_position.tick_upper_index,
        pool_state.tick_spacing,
        tick_upper_state,
    )?;

    if flip_tick_lower {
        let mut tick_array_lower = tick_array_lower_loader.load_mut()?;
        tick_array_lower.update_initialized_tick_count(false)?;
        if tick_array_lower.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(
                tickarray_bitmap_extension,
                tick_array_lower.start_tick_index,
            )?;
        }
    }
    if flip_tick_upper {
        let mut tick_array_upper = tick_array_upper_loader.load_mut()?;
        tick_array_upper.update_initialized_tick_count(false)?;
        if tick_array_upper.initialized_tick_count == 0 {
            pool_state.flip_tick_array_bit(
                tickarray_bitmap_extension,
                tick_array_upper.start_tick_index,
            )?;
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

    Ok((amount_0, amount_1))
}

pub fn collect_rewards<'a, 'b, 'c, 'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    remaining_accounts: &[&'info AccountInfo<'info>],
    token_program: &'b Program<'info, Token>,
    token_program_2022: Option<AccountInfo<'info>>,
    personal_position_state: &mut PersonalPositionState,
    need_reward_mint: bool,
) -> Result<[u64; REWARD_NUM]> {
    let mut reward_amounts: [u64; REWARD_NUM] = [0, 0, 0];
    if !pool_state_loader
        .load()?
        .get_status_by_bit(PoolStatusBitIndex::CollectReward)
    {
        return Ok(reward_amounts);
    }
    let mut reward_group_account_num = 3;
    if !need_reward_mint {
        reward_group_account_num = reward_group_account_num - 1
    }
    check_required_accounts_length(
        pool_state_loader,
        remaining_accounts,
        reward_group_account_num,
    )?;

    let remaining_accounts_len = remaining_accounts.len();
    let mut remaining_accounts = remaining_accounts.iter();
    for i in 0..remaining_accounts_len / reward_group_account_num {
        let reward_token_vault = InterfaceAccount::<token_interface::TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?;
        let recipient_token_account = InterfaceAccount::<token_interface::TokenAccount>::try_from(
            remaining_accounts.next().unwrap(),
        )?;

        let mut reward_vault_mint: Option<Box<InterfaceAccount<Mint>>> = None;
        if need_reward_mint {
            reward_vault_mint = Some(Box::new(InterfaceAccount::<Mint>::try_from(
                remaining_accounts.next().unwrap(),
            )?));
        }
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
                &reward_token_vault.to_account_info(),
                &recipient_token_account.to_account_info(),
                reward_vault_mint,
                &token_program,
                token_program_2022.clone(),
                transfer_amount,
            )?;
        }
        reward_amounts[i] = transfer_amount
    }

    Ok(reward_amounts)
}

fn check_required_accounts_length(
    pool_state_loader: &AccountLoader<PoolState>,
    remaining_accounts: &[&AccountInfo],
    reward_group_account_num: usize,
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    let mut valid_reward_count = 0;
    for item in pool_state.reward_infos {
        if item.initialized() {
            valid_reward_count = valid_reward_count + 1;
        }
    }
    let remaining_accounts_len = remaining_accounts.len();
    if remaining_accounts_len != valid_reward_count * reward_group_account_num {
        return err!(ErrorCode::InvalidRewardInputAccountNumber);
    }
    Ok(())
}

pub fn check_unclaimed_fees_and_vault(
    pool_state_loader: &AccountLoader<PoolState>,
    token_vault_0: &AccountInfo,
    token_vault_1: &AccountInfo,
) -> Result<()> {
    let token_vault_0_amount = spl_token_2022::extension::StateWithExtensions::<
        spl_token_2022::state::Account,
    >::unpack(token_vault_0.try_borrow_data()?.deref())?
    .base
    .amount;

    let token_vault_1_amount = spl_token_2022::extension::StateWithExtensions::<
        spl_token_2022::state::Account,
    >::unpack(token_vault_1.try_borrow_data()?.deref())?
    .base
    .amount;

    let pool_state = &mut pool_state_loader.load_mut()?;

    let unclaimed_fee_token_0 = pool_state
        .total_fees_token_0
        .checked_sub(pool_state.total_fees_claimed_token_0)
        .unwrap();
    let unclaimed_fee_token_1 = pool_state
        .total_fees_token_1
        .checked_sub(pool_state.total_fees_claimed_token_1)
        .unwrap();

    if (unclaimed_fee_token_0 >= token_vault_0_amount && token_vault_0_amount != 0)
        || (unclaimed_fee_token_1 >= token_vault_1_amount && token_vault_1_amount != 0)
    {
        pool_state.set_status_by_bit(PoolStatusBitIndex::CollectFee, PoolStatusBitFlag::Disable);
    }
    Ok(())
}
