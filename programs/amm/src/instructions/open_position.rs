use crate::error::ErrorCode;
use crate::libraries::{liquidity_amounts, liquidity_math};
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token;
use anchor_spl::token::{Mint, Token, TokenAccount};
use mpl_token_metadata::{instruction::create_metadata_accounts_v2, state::Creator};
use spl_token::instruction::AuthorityType;
use std::cell::RefMut;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::Deref;

pub struct AddLiquidityParam<'b, 'info> {
    /// Pays to mint liquidity
    pub payer: &'b Signer<'info>,

    /// The token account spending token_0 to mint the position
    pub token_account_0: &'b mut Box<Account<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
    pub token_account_1: &'b mut Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    pub token_vault_0: &'b mut Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    pub token_vault_1: &'b mut Box<Account<'info, TokenAccount>>,

    /// The bitmap storing initialization state of the lower tick
    pub tick_array_lower: &'b AccountLoader<'info, TickArrayState>,

    /// The bitmap storing initialization state of the upper tick
    pub tick_array_upper: &'b AccountLoader<'info, TickArrayState>,

    /// The position into which liquidity is minted
    pub protocol_position: &'b mut Box<Account<'info, ProtocolPositionState>>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct OpenPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = pool_state.key(),
        payer = payer
    )]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    /// Token account where position NFT will be minted
    #[account(
        init,
        associated_token::mint = position_nft_mint,
        associated_token::authority = position_nft_owner,
        payer = payer
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub metadata_account: UncheckedAccount<'info>,

    /// Add liquidity for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Store the information of market marking in range
    #[account(
        init_if_needed,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        bump,
        payer = payer,
        space = ProtocolPositionState::LEN
    )]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_lower_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK:Account to store data for the position's upper tick
    #[account(
        mut,
        seeds = [
            TICK_ARRAY_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_array_upper_start_index.to_be_bytes(),
        ],
        bump,
    )]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// personal position state
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = payer,
        space = PersonalPositionState::LEN
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// The token_0 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<Account<'info, TokenAccount>>,

    /// The token_1 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,

    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// Program to create NFT metadata
    /// CHECK: Metadata program address constraint applied
    #[account(address = mpl_token_metadata::ID)]
    pub metadata_program: UncheckedAccount<'info>,
}

pub fn open_position<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, OpenPosition<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    check_ticks_order(tick_lower_index, tick_upper_index)?;
    check_tick_array_start_index(
        tick_array_lower_start_index,
        tick_lower_index,
        pool_state.tick_spacing,
    )?;
    check_tick_array_start_index(
        tick_array_upper_start_index,
        tick_upper_index,
        pool_state.tick_spacing,
    )?;

    let tick_array_lower_state = TickArrayState::get_or_create_tick_array(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.tick_array_lower.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.pool_state,
        tick_array_lower_start_index,
    )?;

    let tick_array_upper_state = if tick_array_lower_start_index == tick_array_upper_start_index {
        AccountLoader::<TickArrayState>::try_from(&ctx.accounts.tick_array_upper.to_account_info())?
    } else {
        TickArrayState::get_or_create_tick_array(
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.tick_array_upper.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            &ctx.accounts.pool_state,
            tick_array_upper_start_index,
        )?
    };

    // check if protocol position is initilized
    if ctx.accounts.protocol_position.bump == 0 {
        let protocol_position = &mut ctx.accounts.protocol_position;
        protocol_position.bump = *ctx.bumps.get("protocol_position").unwrap();
        protocol_position.pool_id = ctx.accounts.pool_state.key();
    }

    let mut add_liquidity_context = AddLiquidityParam {
        payer: &ctx.accounts.payer,
        token_account_0: &mut ctx.accounts.token_account_0,
        token_account_1: &mut ctx.accounts.token_account_1,
        token_vault_0: &mut ctx.accounts.token_vault_0,
        token_vault_1: &mut ctx.accounts.token_vault_1,
        tick_array_lower: &tick_array_lower_state,
        tick_array_upper: &tick_array_upper_state,
        protocol_position: &mut ctx.accounts.protocol_position,
        token_program: ctx.accounts.token_program.clone(),
    };

    let (amount_0, amount_1) = add_liquidity(
        &mut add_liquidity_context,
        &mut pool_state,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower_index,
        tick_upper_index,
    )?;

    let seeds = [
        &POOL_SEED.as_bytes(),
        pool_state.amm_config.as_ref(),
        pool_state.token_mint_0.as_ref(),
        pool_state.token_mint_1.as_ref(),
        &[pool_state.bump] as &[u8],
    ];

    create_nft_with_metadata(
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.position_nft_mint.to_account_info(),
        &ctx.accounts.position_nft_account.to_account_info(),
        &ctx.accounts.metadata_account.to_account_info(),
        &ctx.accounts.metadata_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.rent.to_account_info(),
        seeds,
    )?;

    let personal_position = &mut ctx.accounts.personal_position;
    personal_position.bump = *ctx.bumps.get("personal_position").unwrap();
    personal_position.nft_mint = ctx.accounts.position_nft_mint.key();
    personal_position.pool_id = ctx.accounts.pool_state.key();
    personal_position.tick_lower_index = tick_lower_index;
    personal_position.tick_upper_index = tick_upper_index;

    let updated_protocol_position = add_liquidity_context.protocol_position;
    personal_position.fee_growth_inside_0_last_x64 =
        updated_protocol_position.fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last_x64 =
        updated_protocol_position.fee_growth_inside_1_last_x64;

    // update rewards, must update before update liquidity
    personal_position.update_rewards(updated_protocol_position.reward_growth_inside)?;
    personal_position.liquidity = liquidity;

    emit!(CreatePersonalPositionEvent {
        pool_state: ctx.accounts.pool_state.key(),
        minter: ctx.accounts.payer.key(),
        nft_owner: ctx.accounts.position_nft_owner.key(),
        tick_lower_index: tick_lower_index,
        tick_upper_index: tick_upper_index,
        liquidity: liquidity,
        deposit_amount_0: amount_0,
        deposit_amount_1: amount_1,
    });

    Ok(())
}

/// Add liquidity to an initialized pool
pub fn add_liquidity<'b, 'info>(
    context: &mut AddLiquidityParam<'b, 'info>,
    pool_state: &mut RefMut<PoolState>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
) -> Result<(u64, u64)> {
    assert!(liquidity > 0);
    let balance_0_before = context.token_vault_0.amount;
    let balance_1_before = context.token_vault_1.amount;

    let mut tick_array_lower = context.tick_array_lower.load_mut()?;
    let tick_lower_state =
        tick_array_lower.get_tick_state_mut(tick_lower_index, pool_state.tick_spacing as i32)?;

    let mut tick_array_upper = context.tick_array_upper.load_mut()?;
    let tick_upper_state =
        tick_array_upper.get_tick_state_mut(tick_upper_index, pool_state.tick_spacing as i32)?;

    tick_lower_state.tick = tick_lower_index;
    tick_upper_state.tick = tick_upper_index;

    let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
        i128::try_from(liquidity).unwrap(),
        pool_state,
        context.protocol_position,
        tick_lower_state,
        tick_upper_state,
    )?;

    if flip_tick_lower {
        let before_init_tick_count = tick_array_lower.initialized_tick_count;
        tick_array_lower.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_lower.start_tick_index)?;
        }
    }
    if flip_tick_upper {
        let before_init_tick_count = tick_array_upper.initialized_tick_count;
        tick_array_upper.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_upper.start_tick_index)?;
        }
    }

    let amount_0 = amount_0_int as u64;
    let amount_1 = amount_1_int as u64;
    if amount_0 > 0 {
        transfer_from_user_to_pool_vault(
            &context.payer,
            &context.token_account_0,
            &context.token_vault_0,
            &context.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        transfer_from_user_to_pool_vault(
            &context.payer,
            &context.token_account_1,
            &context.token_vault_1,
            &context.token_program,
            amount_1,
        )?;
    }

    context.token_vault_0.reload()?;
    context.token_vault_1.reload()?;
    require_eq!(amount_0, context.token_vault_0.amount - balance_0_before);
    require_eq!(amount_1, context.token_vault_1.amount - balance_1_before);
    #[cfg(feature = "enable-log")]
    msg!(
        "amount_0:{},amount_1:{},amount_0_max:{},amount_1_max:{}",
        amount_0,
        amount_1,
        amount_0_max,
        amount_1_max
    );
    require!(
        amount_0 <= amount_0_max && amount_1 <= amount_1_max,
        ErrorCode::PriceSlippageCheck
    );

    Ok((amount_0, amount_1))
}

pub fn modify_position<'info>(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut Box<Account<'info, ProtocolPositionState>>,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
) -> Result<(i64, i64, bool, bool)> {
    let (flip_tick_lower, flip_tick_upper) = update_position(
        liquidity_delta,
        pool_state,
        protocol_position_state,
        tick_lower_state,
        tick_upper_state,
    )?;
    let mut amount_0 = 0;
    let mut amount_1 = 0;

    if liquidity_delta != 0 {
        (amount_0, amount_1) = liquidity_amounts::get_amounts_delta_signed(
            pool_state.tick_current,
            pool_state.sqrt_price_x64,
            tick_lower_state.tick,
            tick_upper_state.tick,
            liquidity_delta,
        )?;
        if pool_state.tick_current >= tick_lower_state.tick
            && pool_state.tick_current < tick_upper_state.tick
        {
            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
    }

    Ok((amount_0, amount_1, flip_tick_lower, flip_tick_upper))
}

/// Updates a position with the given liquidity delta and tick
pub fn update_position<'info>(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut Box<Account<'info, ProtocolPositionState>>,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
) -> Result<(bool, bool)> {
    let clock = Clock::get()?;
    let updated_reward_infos = pool_state.update_reward_infos(clock.unix_timestamp as u64)?;
    let reward_growths_outside_x64 = RewardInfo::get_reward_growths(&updated_reward_infos);
    #[cfg(feature = "enable-log")]
    msg!(
        "update_position, pool reward_growths_outside_x64:{:?}",
        reward_growths_outside_x64
    );

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower_state.update(
            pool_state.tick_current,
            liquidity_delta,
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            false,
            reward_growths_outside_x64,
        )?;
        flipped_upper = tick_upper_state.update(
            pool_state.tick_current,
            liquidity_delta,
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            true,
            reward_growths_outside_x64,
        )?;
        #[cfg(feature = "enable-log")]
        msg!(
            "tick_upper.reward_growths_outside_x64:{:?}, tick_lower.reward_growths_outside_x64:{:?}",
            identity(tick_upper_state.reward_growths_outside_x64),
            identity(tick_lower_state.reward_growths_outside_x64)
        );
    }
    // Update fees accrued to the position
    let (fee_growth_inside_0_x64, fee_growth_inside_1_x64) = tick_array::get_fee_growth_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        pool_state.fee_growth_global_0_x64,
        pool_state.fee_growth_global_1_x64,
    );

    // Update reward accrued to the position
    let reward_growths_inside = tick_array::get_reward_growths_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        &updated_reward_infos,
    );
    #[cfg(feature = "enable-log")]
    msg!("reward_growths_inside:{:?}", reward_growths_inside);
    protocol_position_state.update(
        tick_lower_state.tick,
        tick_upper_state.tick,
        liquidity_delta,
        fee_growth_inside_0_x64,
        fee_growth_inside_1_x64,
        reward_growths_inside,
    )?;
    if liquidity_delta < 0 {
        if flipped_lower {
            tick_lower_state.clear();
        }
        if flipped_upper {
            tick_upper_state.clear();
        }
    }
    Ok((flipped_lower, flipped_upper))
}

fn create_nft_with_metadata<'info>(
    payer: &AccountInfo<'info>,
    pool_state_info: &AccountInfo<'info>,
    position_nft_mint: &AccountInfo<'info>,
    position_nft_account: &AccountInfo<'info>,
    metadata_account: &AccountInfo<'info>,
    metadata_program: &AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    rent: AccountInfo<'info>,
    seeds: [&[u8]; 5],
) -> Result<()> {
    // Mint the NFT
    token::mint_to(
        CpiContext::new_with_signer(
            token_program.clone(),
            token::MintTo {
                mint: position_nft_mint.clone(),
                to: position_nft_account.clone(),
                authority: pool_state_info.clone(),
            },
            &[&seeds[..]],
        ),
        1,
    )?;

    let create_metadata_ix = create_metadata_accounts_v2(
        metadata_program.key(),
        metadata_account.key(),
        position_nft_mint.key(),
        pool_state_info.key(),
        payer.key(),
        pool_state_info.key(),
        String::from("Raydium AMM V3 Positions"),
        String::from(""),
        String::from(""),
        Some(vec![Creator {
            address: pool_state_info.key(),
            verified: true,
            share: 100,
        }]),
        0,
        true,
        false,
        None,
        None,
    );
    solana_program::program::invoke_signed(
        &create_metadata_ix,
        &[
            metadata_account.clone(),
            position_nft_mint.clone(),
            payer.to_account_info().clone(),
            pool_state_info.clone(),
            system_program.clone(),
            rent.clone(),
        ],
        &[&seeds[..]],
    )?;

    // Disable minting
    token::set_authority(
        CpiContext::new_with_signer(
            token_program.clone(),
            token::SetAuthority {
                current_authority: pool_state_info.clone(),
                account_or_mint: position_nft_mint.clone(),
            },
            &[&seeds[..]],
        ),
        AuthorityType::MintTokens,
        None,
    )?;
    Ok(())
}
