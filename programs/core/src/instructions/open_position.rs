use crate::error::ErrorCode;
use crate::libraries::{liquidity_amounts, liquidity_math, sqrt_price_math, tick_math};
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token;
use anchor_spl::token::{Mint, Token, TokenAccount};
use mpl_token_metadata::{instruction::create_metadata_accounts_v2, state::Creator};
use spl_token::instruction::AuthorityType;
use std::ops::{Deref, DerefMut};

pub struct MintParam<'b, 'info> {
    /// Pays to mint liquidity
    pub payer: &'b Signer<'info>,

    /// The token account spending token_0 to mint the position
    pub token_account_0: &'b mut Account<'info, TokenAccount>,

    /// The token account spending token_1 to mint the position
    pub token_account_1: &'b mut Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_0
    pub token_vault_0: &'b mut Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_1
    pub token_vault_1: &'b mut Account<'info, TokenAccount>,

    /// Mint liquidity for this pool
    pub pool_state: &'b mut Account<'info, PoolState>,

    /// The lower tick boundary of the position
    pub tick_lower: &'b mut Account<'info, TickState>,

    /// The upper tick boundary of the position
    pub tick_upper: &'b mut Account<'info, TickState>,

    /// The bitmap storing initialization state of the lower tick
    pub bitmap_lower: &'b AccountLoader<'info, TickBitmapState>,

    /// The bitmap storing initialization state of the upper tick
    pub bitmap_upper: &'b AccountLoader<'info, TickBitmapState>,

    /// The position into which liquidity is minted
    pub protocol_position: &'b mut Account<'info, ProcotolPositionState>,

    /// The program account for the most recent oracle observation, at index = pool.observation_index
    pub last_observation: &'b mut Account<'info, ObservationState>,

    /// The next observation state
    pub next_observation: &'b mut Account<'info, ObservationState>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,word_lower_index:i16,word_upper_index:i16)]
pub struct OpenPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Receives the position NFT
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub position_nft_owner: UncheckedAccount<'info>,

    /// The program account acting as the core liquidity custodian for token holder, and as
    /// mint authority of the position NFT
    #[account(address = pool_state.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = amm_config,
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

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
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
        space = ProcotolPositionState::LEN
    )]
    pub protocol_position: Box<Account<'info, ProcotolPositionState>>,

    /// Account to store data for the position's lower tick
    #[account(
        init_if_needed,
        seeds = [
            TICK_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes()
        ],
        bump,
        payer = payer,
        space = TickState::LEN
    )]
    pub tick_lower: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    #[account(
        init_if_needed,
        seeds = [
            TICK_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_upper_index.to_be_bytes()
        ],
        bump,
        payer = payer,
        space = TickState::LEN
    )]
    pub tick_upper: Box<Account<'info, TickState>>,

    /// CHECK: Account to mark the lower tick as initialized
    #[account(mut)]
    pub tick_bitmap_lower: UncheckedAccount<'info>,

    /// CHECK:Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_bitmap_upper: UncheckedAccount<'info>,

    /// Metadata for the tokenized position
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = payer,
        space = PersonalPositionState::LEN
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// The token account spending token_0 to mint the position
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<Account<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// The latest observation state
    #[account(mut)]
    pub last_observation: Box<Account<'info, ObservationState>>,

    /// The next observation state
    #[account(mut)]
    pub next_observation: Box<Account<'info, ObservationState>>,

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
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower: i32,
    tick_upper: i32,
    word_pos_lower: i16,
    word_pos_upper: i16,
) -> Result<()> {
    assert!(tick_lower < tick_upper);
    assert!(word_pos_lower <= word_pos_upper);
    if ctx.accounts.protocol_position.bump == 0 {
        let position_account = &mut ctx.accounts.protocol_position;
        position_account.bump = *ctx.bumps.get("protocol_position").unwrap();
    }

    if ctx.accounts.tick_lower.bump == 0 {
        let tick_state = ctx.accounts.tick_lower.as_mut();
        tick_state.initialize(
            *ctx.bumps.get("tick_lower").unwrap(),
            tick_lower,
            ctx.accounts.pool_state.tick_spacing,
        )?;
    }

    if ctx.accounts.tick_upper.bump == 0 {
        let tick_state = ctx.accounts.tick_upper.as_mut();
        tick_state.initialize(
            *ctx.bumps.get("tick_upper").unwrap(),
            tick_upper,
            ctx.accounts.pool_state.tick_spacing,
        )?;
    }

    let bitmap_lower_state = TickBitmapState::get_or_create_tick_bitmap(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.tick_bitmap_lower.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.pool_state.key(),
        word_pos_lower,
        ctx.accounts.pool_state.tick_spacing,
    )?;

    let bitmap_upper_state = if word_pos_lower == word_pos_upper {
        AccountLoader::<TickBitmapState>::try_from(
            &ctx.accounts.tick_bitmap_upper.to_account_info(),
        )?
    } else {
        TickBitmapState::get_or_create_tick_bitmap(
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.tick_bitmap_upper.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.pool_state.key(),
            word_pos_upper,
            ctx.accounts.pool_state.tick_spacing,
        )?
    };

    // Validate addresses manually, as constraint checks are not applied to internal calls
    let pool_state_info = ctx.accounts.pool_state.to_account_info();
    let pool_state_clone = ctx.accounts.pool_state.clone();
    let tick_lower = ctx.accounts.tick_lower.tick;
    let tick_upper = ctx.accounts.tick_upper.tick;
    require_keys_eq!(
        *ctx.accounts.tick_bitmap_lower.to_account_info().owner,
        crate::id()
    );

    // let aa = ctx.accounts.bitmap_lower_state.load_mut()?;
    let mut accounts = MintParam {
        payer: &ctx.accounts.payer,
        token_account_0: ctx.accounts.token_account_0.as_mut(),
        token_account_1: ctx.accounts.token_account_1.as_mut(),
        token_vault_0: ctx.accounts.token_vault_0.as_mut(),
        token_vault_1: ctx.accounts.token_vault_1.as_mut(),
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower: ctx.accounts.tick_lower.as_mut(),
        tick_upper: ctx.accounts.tick_upper.as_mut(),
        bitmap_lower: &bitmap_lower_state,
        bitmap_upper: &bitmap_upper_state,
        protocol_position: ctx.accounts.protocol_position.as_mut(),
        last_observation: ctx.accounts.last_observation.as_mut(),
        next_observation: ctx.accounts.next_observation.as_mut(),
        token_program: ctx.accounts.token_program.clone(),
    };

    let (liquidity, amount_0, amount_1) = add_liquidity(
        &mut accounts,
        ctx.remaining_accounts,
        amount_0_desired,
        amount_1_desired,
        amount_0_min,
        amount_1_min,
        tick_lower,
        tick_upper,
    )?;

    let amm_config_key =  ctx.accounts.amm_config.key();
    let token_mint_0 = pool_state_clone.token_mint_0.key();
    let token_mint_1 = pool_state_clone.token_mint_1.key();
    let seeds = [
        &POOL_SEED.as_bytes(),
        amm_config_key.as_ref(),
        token_mint_0.as_ref(),
        token_mint_1.as_ref(),
        &[pool_state_clone.bump] as &[u8],
    ];
    // Mint the NFT
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info().clone(),
            token::MintTo {
                mint: ctx.accounts.position_nft_mint.to_account_info().clone(),
                to: ctx.accounts.position_nft_account.to_account_info().clone(),
                authority: pool_state_info.clone(),
            },
            &[&seeds[..]],
        ),
        1,
    )?;

    let create_metadata_ix = create_metadata_accounts_v2(
        ctx.accounts.metadata_program.key(),
        ctx.accounts.metadata_account.key(),
        ctx.accounts.position_nft_mint.key(),
        pool_state_info.key(),
        ctx.accounts.payer.key(),
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
            ctx.accounts.metadata_account.to_account_info().clone(),
            ctx.accounts.position_nft_mint.to_account_info().clone(),
            ctx.accounts.payer.to_account_info().clone(),
            pool_state_info.clone(), // mint and update authority
            ctx.accounts.system_program.to_account_info().clone(),
            ctx.accounts.rent.to_account_info().clone(),
        ],
        &[&seeds[..]],
    )?;

    // Disable minting
    token::set_authority(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info().clone(),
            token::SetAuthority {
                current_authority: pool_state_info.clone(),
                account_or_mint: ctx.accounts.position_nft_mint.to_account_info().clone(),
            },
            &[&seeds[..]],
        ),
        AuthorityType::MintTokens,
        None,
    )?;

    // Write tokenized position metadata
    let personal_position = &mut ctx.accounts.personal_position;
    personal_position.bump = *ctx.bumps.get("personal_position").unwrap();
    personal_position.nft_mint = ctx.accounts.position_nft_mint.key();
    personal_position.pool_id = pool_state_info.key();

    personal_position.tick_lower = tick_lower; // can read from core position
    personal_position.tick_upper = tick_upper;
    personal_position.liquidity = liquidity;

    let updated_core_position = accounts.protocol_position;
    personal_position.fee_growth_inside_0_last = updated_core_position.fee_growth_inside_0_last;
    personal_position.fee_growth_inside_1_last = updated_core_position.fee_growth_inside_1_last;
    personal_position.update_rewards(updated_core_position.reward_growth_inside)?;

    emit!(CreatePersonalPositionEvent {
        pool_state: pool_state_info.key(),
        minter: ctx.accounts.payer.key(),
        nft_owner: ctx.accounts.position_nft_owner.key(),
        tick_lower: tick_lower,
        tick_upper: tick_upper,
        liquidity: liquidity,
        deposit_amount_0: amount_0,
        deposit_amount_1: amount_1,
    });

    Ok(())
}

/// Add liquidity to an initialized pool
///
/// # Arguments
///
/// * `accounts` - Accounts to mint core liquidity
/// * `amount_0_desired` - Desired amount of token_0 to be spent
/// * `amount_1_desired` - Desired amount of token_1 to be spent
/// * `amount_0_min` - The minimum amount of token_0 to spend, which serves as a slippage check
/// * `amount_1_min` - The minimum amount of token_1 to spend, which serves as a slippage check
/// * `tick_lower` - The lower tick bound for the position
/// * `tick_upper` - The upper tick bound for the position
///
pub fn add_liquidity<'b, 'info>(
    accounts: &mut MintParam<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<(u128, u64, u64)> {
    let sqrt_price_x64 = accounts.pool_state.sqrt_price_x64;

    let sqrt_ratio_a_x64 = tick_math::get_sqrt_ratio_at_tick(tick_lower)?;
    let sqrt_ratio_b_x64 = tick_math::get_sqrt_ratio_at_tick(tick_upper)?;
    let liquidity = liquidity_amounts::get_liquidity_for_amounts(
        sqrt_price_x64,
        sqrt_ratio_a_x64,
        sqrt_ratio_b_x64,
        amount_0_desired,
        amount_1_desired,
    );

    let balance_0_before = accounts.token_vault_0.amount;
    let balance_1_before = accounts.token_vault_1.amount;

    mint(accounts, remaining_accounts, liquidity)?;

    accounts.token_vault_0.reload()?;
    accounts.token_vault_1.reload()?;
    let amount_0 = accounts.token_vault_0.amount - balance_0_before;
    let amount_1 = accounts.token_vault_1.amount - balance_1_before;
    require!(
        amount_0 >= amount_0_min && amount_1 >= amount_1_min,
        ErrorCode::PriceSlippageCheck
    );

    Ok((liquidity, amount_0, amount_1))
}

pub fn mint<'b, 'info>(
    ctx: &mut MintParam<'b, 'info>,
    remaining_accounts: &[AccountInfo<'info>],
    liquidity: u128,
) -> Result<()> {
    assert!(ctx.token_vault_0.key() == ctx.pool_state.token_vault_0);
    assert!(ctx.token_vault_1.key() == ctx.pool_state.token_vault_1);
    ctx.pool_state.validate_tick_address(
        &ctx.tick_lower.key(),
        ctx.tick_lower.bump,
        ctx.tick_lower.tick,
    )?;
    ctx.pool_state.validate_tick_address(
        &ctx.tick_upper.key(),
        ctx.tick_upper.bump,
        ctx.tick_upper.tick,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_lower.key(),
        ctx.bitmap_lower.load()?.bump,
        tick_bitmap::position(ctx.tick_lower.tick / ctx.pool_state.tick_spacing as i32).word_pos,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_upper.key(),
        ctx.bitmap_upper.load()?.bump,
        tick_bitmap::position(ctx.tick_upper.tick / ctx.pool_state.tick_spacing as i32).word_pos,
    )?;

    ctx.pool_state.validate_protocol_position_address(
        &ctx.protocol_position.key(),
        ctx.protocol_position.bump,
        ctx.tick_lower.tick,
        ctx.tick_upper.tick,
    )?;
    ctx.pool_state.validate_observation_address(
        &ctx.last_observation.key(),
        ctx.last_observation.bump,
        false,
    )?;
    assert!(liquidity > 0);

    let (amount_0_int, amount_1_int) = _modify_position(
        i128::try_from(liquidity).unwrap(),
        ctx.pool_state,
        ctx.protocol_position,
        ctx.tick_lower,
        ctx.tick_upper,
        ctx.bitmap_lower,
        ctx.bitmap_upper,
        ctx.last_observation,
        ctx.next_observation,
        remaining_accounts,
    )?;
    // msg!(
    //     "amount_0_init:{},amount_1_init:{}",
    //     amount_0_int,
    //     amount_1_int
    // );
    let amount_0 = amount_0_int as u64;
    let amount_1 = amount_1_int as u64;

    if amount_0 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.payer,
            &ctx.token_account_0,
            &ctx.token_vault_0,
            &ctx.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.payer,
            &ctx.token_account_1,
            &ctx.token_vault_1,
            &ctx.token_program,
            amount_1,
        )?;
    }

    Ok(())
}

/// Credit or debit liquidity to a position, and find the amount of token_0 and token_1
/// required to produce this change.
/// Returns amount of token_0 and token_1 owed to the pool, negative if the pool should
/// pay the recipient.
///
/// # Arguments
///
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Holds the initialization state of the lower tick
/// * `bitmap_upper` - Holds the initialization state of the upper tick
/// * `last_observation_state` - The last written oracle observation, having index = pool.observation_index.
/// This condition must be externally tracked.
/// * `next_observation_state` - The observation account following `last_observation_state`. Becomes equal
/// to last_observation_state when cardinality is 1.
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _modify_position<'info>(
    liquidity_delta: i128,
    pool_state: &mut Account<'info, PoolState>,
    position_state: &mut Account<'info, ProcotolPositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
    last_observation_state: &mut Account<'info, ObservationState>,
    next_observation_state: &mut Account<'info, ObservationState>,
    _remaining_accounts: &[AccountInfo<'info>],
) -> Result<(i64, i64)> {
    crate::check_ticks(tick_lower_state.tick, tick_upper_state.tick)?;

    _update_position(
        liquidity_delta,
        pool_state,
        last_observation_state,
        position_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower,
        bitmap_upper,
    )?;

    let mut amount_0 = 0;
    let mut amount_1 = 0;

    let tick_lower = tick_lower_state.tick;
    let tick_upper = tick_upper_state.tick;

    if liquidity_delta != 0 {
        if pool_state.tick < tick_lower {
            // current tick is below the passed range; liquidity can only become in range by crossing from left to
            // right, when we'll need _more_ token_0 (it's becoming more valuable) so user must provide it
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        } else if pool_state.tick < tick_upper {
            // current tick is inside the passed range
            // write oracle observation
            let timestamp = oracle::_block_timestamp();
            let partition_current_timestamp = timestamp / 14;
            let partition_last_timestamp = last_observation_state.block_timestamp / 14;

            let observation = if partition_current_timestamp > partition_last_timestamp {
                pool_state.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation_state.bump,
                    true,
                )?;
                next_observation_state.deref_mut()
            } else {
                last_observation_state.deref_mut()
            };

            pool_state.observation_cardinality_next = observation.update(
                timestamp,
                pool_state.tick,
                pool_state.liquidity,
                pool_state.observation_cardinality,
                pool_state.observation_cardinality_next,
            );
            pool_state.observation_index = observation.index;

            // Both Δtoken_0 and Δtoken_1 will be needed in current price
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                pool_state.sqrt_price_x64,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                pool_state.sqrt_price_x64,
                liquidity_delta,
            );
            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
        // current tick is above the range
        else {
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        }
    }

    Ok((amount_0, amount_1))
}

/// Updates a position with the given liquidity delta
///
/// # Arguments
///
/// * `pool_state` - Current pool state
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Bitmap account for the lower tick
/// * `bitmap_upper` - Bitmap account for the upper tick, if it is different from
/// `bitmap_lower`
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _update_position<'info>(
    liquidity_delta: i128,
    pool_state: &mut Account<'info, PoolState>,
    last_observation_state: &mut Account<ObservationState>,
    position_state: &mut Account<'info, ProcotolPositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
) -> Result<()> {
    let clock = Clock::get()?;
    let updated_reward_infos = pool_state.update_reward_infos(clock.unix_timestamp as u64)?;
    let reward_growths_outside = RewardInfo::to_reward_growths(&updated_reward_infos);
    #[cfg(feature = "enable-log")]
    msg!(
        "_update_position: update_rewared_info:{:?}",
        reward_growths_outside
    );
    let tick_lower = tick_lower_state.deref_mut();
    let tick_upper = tick_upper_state.deref_mut();

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        let time = oracle::_block_timestamp();
        let (tick_cumulative, seconds_per_liquidity_cumulative_x64) =
            last_observation_state.observe_latest(time, pool_state.tick, pool_state.liquidity);

        let max_liquidity_per_tick =
            tick_spacing_to_max_liquidity_per_tick(pool_state.tick_spacing as i32);

        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0,
            pool_state.fee_growth_global_1,
            seconds_per_liquidity_cumulative_x64,
            tick_cumulative,
            time,
            false,
            max_liquidity_per_tick,
            reward_growths_outside,
        )?;
        flipped_upper = tick_upper.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0,
            pool_state.fee_growth_global_1,
            seconds_per_liquidity_cumulative_x64,
            tick_cumulative,
            time,
            true,
            max_liquidity_per_tick,
            reward_growths_outside,
        )?;
        #[cfg(feature = "enable-log")]
        msg!(
            "tick_upper.reward_growths_outside:{:?}, tick_lower.reward_growths_outside:{:?}",
            tick_upper.reward_growths_outside,
            tick_lower.reward_growths_outside
        );
        if flipped_lower {
            let bit_pos = ((tick_lower.tick / pool_state.tick_spacing as i32) % 256) as u8; // rightmost 8 bits
            bitmap_lower.load_mut()?.flip_bit(bit_pos);
        }
        if flipped_upper {
            let bit_pos = ((tick_upper.tick / pool_state.tick_spacing as i32) % 256) as u8;
            if bitmap_lower.key() == bitmap_upper.key() {
                bitmap_lower.load_mut()?.flip_bit(bit_pos);
            } else {
                bitmap_upper.load_mut()?.flip_bit(bit_pos);
            }
        }
    }
    // Update fees accrued to the position
    let (fee_growth_inside_0_x64, fee_growth_inside_1_x64) = tick::get_fee_growth_inside(
        tick_lower.deref(),
        tick_upper.deref(),
        pool_state.tick,
        pool_state.fee_growth_global_0,
        pool_state.fee_growth_global_1,
    );

    // Update reward accrued to the position
    let reward_growths_inside = tick::get_reward_growths_inside(
        tick_lower.deref(),
        tick_upper.deref(),
        pool_state.tick,
        &updated_reward_infos,
    );

    position_state.update(
        liquidity_delta,
        fee_growth_inside_0_x64,
        fee_growth_inside_1_x64,
        reward_growths_inside,
    )?;

    // Deallocate the tick accounts if they get un-initialized
    // A tick is un-initialized on flip if liquidity_delta is negative
    if liquidity_delta < 0 {
        if flipped_lower {
            tick_lower.clear();
        }
        if flipped_upper {
            tick_upper.clear();
        }
    }
    Ok(())
}
