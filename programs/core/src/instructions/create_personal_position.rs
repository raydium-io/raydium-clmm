use crate::error::ErrorCode;
use crate::libraries::{liquidity_amounts, liquidity_math, sqrt_price_math, tick_math};
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token;
use anchor_spl::token::{Mint, Token, TokenAccount};
use std::ops::{Deref, DerefMut};
pub struct MintParam<'b, 'info> {
    /// Pays to mint liquidity
    pub minter: &'b Signer<'info>,

    /// The token account spending token_0 to mint the position
    pub token_account_0: &'b mut Account<'info, TokenAccount>,

    /// The token account spending token_1 to mint the position
    pub token_account_1: &'b mut Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_0
    pub token_vault_0: &'b mut Account<'info, TokenAccount>,

    /// The address that holds pool tokens for token_1
    pub token_vault_1: &'b mut Account<'info, TokenAccount>,

    /// Liquidity is minted on behalf of recipient
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub protocol_position_owner: UncheckedAccount<'info>,

    /// Mint liquidity for this pool
    pub pool_state: &'b mut Account<'info, PoolState>,

    /// The lower tick boundary of the position
    pub tick_lower_state: &'b mut Account<'info, TickState>,

    /// The upper tick boundary of the position
    pub tick_upper_state: &'b mut Account<'info, TickState>,

    /// The bitmap storing initialization state of the lower tick
    pub bitmap_lower_state: &'b AccountLoader<'info, TickBitmapState>,

    /// The bitmap storing initialization state of the upper tick
    pub bitmap_upper_state: &'b AccountLoader<'info, TickBitmapState>,

    /// The position into which liquidity is minted
    pub position_state: &'b mut Account<'info, ProcotolPositionState>,

    /// The program account for the most recent oracle observation, at index = pool.observation_index
    pub last_observation_state: &'b mut Account<'info, ObservationState>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CreatePersonalPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub minter: Signer<'info>,

    /// Receives the position NFT
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub position_nft_owner: UncheckedAccount<'info>,

    /// The program account acting as the core liquidity custodian for token holder, and as
    /// mint authority of the position NFT
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = amm_config,
        payer = minter
    )]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    /// Token account where position NFT will be minted
    #[account(
        init,
        associated_token::mint = position_nft_mint,
        associated_token::authority = position_nft_owner,
        payer = minter
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
    #[account(mut)]
    pub protocol_position_state: Box<Account<'info, ProcotolPositionState>>,

    /// Account to store data for the position's lower tick
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// Account to store data for the position's upper tick
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// Account to mark the lower tick as initialized
    #[account(mut)]
    pub bitmap_lower_state: AccountLoader<'info, TickBitmapState>, // remove

    /// Account to mark the upper tick as initialized
    #[account(mut)]
    pub bitmap_upper_state: AccountLoader<'info, TickBitmapState>, // remove

    /// Metadata for the tokenized position
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = minter,
        space = PersonalPositionState::LEN
    )]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

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
    pub last_observation_state: Box<Account<'info, ObservationState>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,

    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn create_personal_position<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CreatePersonalPosition<'info>>,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    // Validate addresses manually, as constraint checks are not applied to internal calls
    let pool_state_info = ctx.accounts.pool_state.to_account_info();
    let tick_lower = ctx.accounts.tick_lower_state.tick;
    let tick_upper = ctx.accounts.tick_upper_state.tick;
    require_keys_eq!(
        *ctx.accounts.bitmap_lower_state.to_account_info().owner,
        crate::id()
    );
    // let aa = ctx.accounts.bitmap_lower_state.load_mut()?;
    let mut accounts = MintParam {
        minter: &ctx.accounts.minter,
        token_account_0: ctx.accounts.token_account_0.as_mut(),
        token_account_1: ctx.accounts.token_account_1.as_mut(),
        token_vault_0: ctx.accounts.token_vault_0.as_mut(),
        token_vault_1: ctx.accounts.token_vault_1.as_mut(),
        protocol_position_owner: UncheckedAccount::try_from(
            ctx.accounts.amm_config.to_account_info(),
        ),
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
        tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
        bitmap_lower_state: &ctx.accounts.bitmap_lower_state,
        bitmap_upper_state: &ctx.accounts.bitmap_upper_state,
        position_state: ctx.accounts.protocol_position_state.as_mut(),
        last_observation_state: ctx.accounts.last_observation_state.as_mut(),
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

    // Mint the NFT
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info().clone(),
            token::MintTo {
                mint: ctx.accounts.position_nft_mint.to_account_info().clone(),
                to: ctx.accounts.position_nft_account.to_account_info().clone(),
                authority: ctx.accounts.amm_config.to_account_info().clone(),
            },
            &[&[&[ctx.accounts.amm_config.bump] as &[u8]]],
        ),
        1,
    )?;

    // Write tokenized position metadata
    let tokenized_position = &mut ctx.accounts.personal_position_state;
    tokenized_position.bump = *ctx.bumps.get("personal_position_state").unwrap();
    tokenized_position.mint = ctx.accounts.position_nft_mint.key();
    tokenized_position.pool_id = pool_state_info.key();

    tokenized_position.tick_lower = tick_lower; // can read from core position
    tokenized_position.tick_upper = tick_upper;
    tokenized_position.liquidity = liquidity;

    let updated_core_position = accounts.position_state;
    tokenized_position.fee_growth_inside_0_last = updated_core_position.fee_growth_inside_0_last;
    tokenized_position.fee_growth_inside_1_last = updated_core_position.fee_growth_inside_1_last;

    emit!(IncreaseLiquidityEvent {
        position_nft_mint: ctx.accounts.position_nft_mint.key(),
        liquidity,
        amount_0,
        amount_1
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
) -> Result<(u64, u64, u64)> {
    let sqrt_price_x32 = accounts.pool_state.sqrt_price;

    let sqrt_ratio_a_x32 = tick_math::get_sqrt_ratio_at_tick(tick_lower)?;
    let sqrt_ratio_b_x32 = tick_math::get_sqrt_ratio_at_tick(tick_upper)?;
    let liquidity = liquidity_amounts::get_liquidity_for_amounts(
        sqrt_price_x32,
        sqrt_ratio_a_x32,
        sqrt_ratio_b_x32,
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
    amount: u64,
) -> Result<()> {
    let pool_state_info = ctx.pool_state.to_account_info();

    assert!(ctx.token_vault_0.key() == ctx.pool_state.token_vault_0);
    assert!(ctx.token_vault_1.key() == ctx.pool_state.token_vault_1);
    ctx.pool_state.validate_tick_address(
        &ctx.tick_lower_state.key(),
        ctx.tick_lower_state.bump,
        ctx.tick_lower_state.tick,
    )?;
    ctx.pool_state.validate_tick_address(
        &ctx.tick_upper_state.key(),
        ctx.tick_upper_state.bump,
        ctx.tick_upper_state.tick,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_lower_state.key(),
        ctx.bitmap_lower_state.load()?.bump,
        tick_bitmap::position(ctx.tick_lower_state.tick / ctx.pool_state.tick_spacing as i32)
            .word_pos,
    )?;
    ctx.pool_state.validate_bitmap_address(
        &ctx.bitmap_upper_state.key(),
        ctx.bitmap_upper_state.load()?.bump,
        tick_bitmap::position(ctx.tick_upper_state.tick / ctx.pool_state.tick_spacing as i32)
            .word_pos,
    )?;

    ctx.pool_state.validate_position_address(
        &ctx.position_state.key(),
        ctx.position_state.bump,
        &ctx.protocol_position_owner.key(),
        ctx.tick_lower_state.tick,
        ctx.tick_upper_state.tick,
    )?;
    ctx.pool_state.validate_observation_address(
        &ctx.last_observation_state.key(),
        ctx.last_observation_state.bump,
        false,
    )?;

    assert!(amount > 0);

    let (amount_0_int, amount_1_int) = _modify_position(
        i64::try_from(amount).unwrap(),
        ctx.pool_state,
        ctx.position_state,
        ctx.tick_lower_state,
        ctx.tick_upper_state,
        ctx.bitmap_lower_state,
        ctx.bitmap_upper_state,
        ctx.last_observation_state,
        remaining_accounts,
    )?;
    msg!(
        "amount_0_int:{},amount_1_int:{}",
        amount_0_int,
        amount_1_int
    );
    let amount_0 = amount_0_int as u64;
    let amount_1 = amount_1_int as u64;

    if amount_0 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.minter,
            &ctx.token_account_0,
            &ctx.token_vault_0,
            &ctx.token_program,
            amount_0,
        )?;
    }
    if amount_1 > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.minter,
            &ctx.token_account_1,
            &ctx.token_vault_1,
            &ctx.token_program,
            amount_1,
        )?;
    }
    emit!(CreatePersonalPositionEvent {
        pool_state: pool_state_info.key(),
        minter: ctx.minter.key(),
        nft_owner: ctx.protocol_position_owner.key(),
        tick_lower: ctx.tick_lower_state.tick,
        tick_upper: ctx.tick_upper_state.tick,
        liquidity: amount,
        deposit_amount_0: amount_0,
        deposit_amount_1: amount_1,
    });

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
    liquidity_delta: i64,
    pool_state: &mut Account<'info, PoolState>,
    position_state: &mut Account<'info, ProcotolPositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
    last_observation_state: &mut Account<'info, ObservationState>,
    remaining_accounts: &[AccountInfo<'info>],
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

            let mut next_observation_state;
            let new_observation = if partition_current_timestamp > partition_last_timestamp {
                next_observation_state =
                    Account::<ObservationState>::try_from(&remaining_accounts[0])?;
                // let next_observation = next_observation_state.deref_mut();
                pool_state.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation_state.bump,
                    true,
                )?;

                next_observation_state.deref_mut()
            } else {
                last_observation_state.deref_mut()
            };

            pool_state.observation_cardinality_next = new_observation.update(
                timestamp,
                pool_state.tick,
                pool_state.liquidity,
                pool_state.observation_cardinality,
                pool_state.observation_cardinality_next,
            );
            pool_state.observation_index = new_observation.index;

            // Both Δtoken_0 and Δtoken_1 will be needed in current price
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                pool_state.sqrt_price,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                pool_state.sqrt_price,
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
    liquidity_delta: i64,
    pool_state: &mut Account<'info, PoolState>,
    last_observation_state: &mut Account<ObservationState>,
    position_state: &mut Account<'info, ProcotolPositionState>,
    tick_lower_state: &mut Account<'info, TickState>,
    tick_upper_state: &mut Account<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
) -> Result<()> {
    let tick_lower = tick_lower_state.deref_mut();
    let tick_upper = tick_upper_state.deref_mut();

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        let time = oracle::_block_timestamp();
        let (tick_cumulative, seconds_per_liquidity_cumulative_x32) =
            last_observation_state.observe_latest(time, pool_state.tick, pool_state.liquidity);

        let max_liquidity_per_tick =
            tick_spacing_to_max_liquidity_per_tick(pool_state.tick_spacing as i32);

        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0,
            pool_state.fee_growth_global_1,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            false,
            max_liquidity_per_tick,
        )?;
        flipped_upper = tick_upper.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0,
            pool_state.fee_growth_global_1,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            true,
            max_liquidity_per_tick,
        )?;

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
    let (fee_growth_inside_0_x32, fee_growth_inside_1_x32) = tick::get_fee_growth_inside(
        tick_lower.deref(),
        tick_upper.deref(),
        pool_state.tick,
        pool_state.fee_growth_global_0,
        pool_state.fee_growth_global_1,
    );
    position_state.update(
        liquidity_delta,
        fee_growth_inside_0_x32,
        fee_growth_inside_1_x32,
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
