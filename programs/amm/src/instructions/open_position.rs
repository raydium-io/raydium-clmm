use crate::error::ErrorCode;
use crate::libraries::liquidity_math;
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
    {
        let pool_state = &mut ctx.accounts.pool_state.load_mut()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
            return err!(ErrorCode::NotApproved);
        }
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

        // Why not use anchor's `init-if-needed` to create?
        // Beacuse `tick_array_lower` and `tick_array_upper` can be the same account, anchor can initialze tick_array_lower but it causes a crash when anchor to initialze the `tick_array_upper`,
        // the problem is variable scope, tick_array_lower_loader not exit to save the discriminator while build tick_array_upper_loader.
        let tick_array_lower_loader = TickArrayState::get_or_create_tick_array(
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.tick_array_lower.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            &ctx.accounts.pool_state,
            tick_array_lower_start_index,
            pool_state.tick_spacing,
        )?;

        let tick_array_upper_loader =
            if tick_array_lower_start_index == tick_array_upper_start_index {
                AccountLoader::<TickArrayState>::try_from(
                    &ctx.accounts.tick_array_upper.to_account_info(),
                )?
            } else {
                TickArrayState::get_or_create_tick_array(
                    ctx.accounts.payer.to_account_info(),
                    ctx.accounts.tick_array_upper.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                    &ctx.accounts.pool_state,
                    tick_array_upper_start_index,
                    pool_state.tick_spacing,
                )?
            };

        // check if protocol position is initilized
        if ctx.accounts.protocol_position.pool_id == Pubkey::default() {
            let protocol_position = &mut ctx.accounts.protocol_position;
            protocol_position.bump = *ctx.bumps.get("protocol_position").unwrap();
            protocol_position.pool_id = ctx.accounts.pool_state.key();
            protocol_position.tick_lower_index = tick_lower_index;
            protocol_position.tick_upper_index = tick_upper_index;
            tick_array_lower_loader
                .load_mut()?
                .get_tick_state_mut(tick_lower_index, i32::from(pool_state.tick_spacing))?
                .tick = tick_lower_index;
            tick_array_upper_loader
                .load_mut()?
                .get_tick_state_mut(tick_upper_index, i32::from(pool_state.tick_spacing))?
                .tick = tick_upper_index;
        }

        let mut add_liquidity_context = AddLiquidityParam {
            payer: &ctx.accounts.payer,
            token_account_0: &mut ctx.accounts.token_account_0,
            token_account_1: &mut ctx.accounts.token_account_1,
            token_vault_0: &mut ctx.accounts.token_vault_0,
            token_vault_1: &mut ctx.accounts.token_vault_1,
            tick_array_lower: &tick_array_lower_loader,
            tick_array_upper: &tick_array_upper_loader,
            protocol_position: &mut ctx.accounts.protocol_position,
            token_program: ctx.accounts.token_program.clone(),
        };

        let mut amount_0: u64 = 0;
        let mut amount_1: u64 = 0;

        if liquidity > 0 {
            (amount_0, amount_1) = add_liquidity(
                &mut add_liquidity_context,
                pool_state,
                liquidity,
                amount_0_max,
                amount_1_max,
                tick_lower_index,
                tick_upper_index,
            )?;
        }

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
        personal_position.update_rewards(updated_protocol_position.reward_growth_inside, false)?;
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
    }

    create_nft_with_metadata(
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.pool_state,
        &ctx.accounts.position_nft_mint.to_account_info(),
        &ctx.accounts.position_nft_account.to_account_info(),
        &ctx.accounts.metadata_account.to_account_info(),
        &ctx.accounts.metadata_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.rent.to_account_info(),
    )?;
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
    let liquidity_before = pool_state.liquidity;
    require_keys_eq!(context.tick_array_lower.load()?.pool_id, pool_state.key());
    require_keys_eq!(context.tick_array_upper.load()?.pool_id, pool_state.key());

    // get tick_state
    let mut tick_lower_state = *context
        .tick_array_lower
        .load_mut()?
        .get_tick_state_mut(tick_lower_index, i32::from(pool_state.tick_spacing))?;
    let mut tick_upper_state = *context
        .tick_array_upper
        .load_mut()?
        .get_tick_state_mut(tick_upper_index, i32::from(pool_state.tick_spacing))?;
    if tick_lower_state.tick == 0 {
        tick_lower_state.tick = tick_lower_index;
    }
    if tick_upper_state.tick == 0 {
        tick_upper_state.tick = tick_upper_index;
    }
    let clock = Clock::get()?;
    let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
        i128::try_from(liquidity).unwrap(),
        pool_state,
        context.protocol_position.as_mut(),
        &mut tick_lower_state,
        &mut tick_upper_state,
        clock.unix_timestamp as u64,
    )?;

    // update tick_state
    context.tick_array_lower.load_mut()?.update_tick_state(
        tick_lower_index,
        i32::from(pool_state.tick_spacing),
        tick_lower_state,
    )?;
    context.tick_array_upper.load_mut()?.update_tick_state(
        tick_upper_index,
        i32::from(pool_state.tick_spacing),
        tick_upper_state,
    )?;

    if flip_tick_lower {
        let mut tick_array_lower = context.tick_array_lower.load_mut()?;
        let before_init_tick_count = tick_array_lower.initialized_tick_count;
        tick_array_lower.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_lower.start_tick_index)?;
        }
    }
    if flip_tick_upper {
        let mut tick_array_upper = context.tick_array_upper.load_mut()?;
        let before_init_tick_count = tick_array_upper.initialized_tick_count;
        tick_array_upper.update_initialized_tick_count(true)?;

        if before_init_tick_count == 0 {
            pool_state.flip_tick_array_bit(tick_array_upper.start_tick_index)?;
        }
    }
    require!(
        amount_0_int > 0 || amount_1_int > 0,
        ErrorCode::ForbidBothZeroForSupplyLiquidity
    );

    let amount_0 = u64::try_from(amount_0_int).unwrap();
    let amount_1 = u64::try_from(amount_1_int).unwrap();

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

    transfer_from_user_to_pool_vault(
        &context.payer,
        &context.token_account_0,
        &context.token_vault_0,
        &context.token_program,
        amount_0,
    )?;

    transfer_from_user_to_pool_vault(
        &context.payer,
        &context.token_account_1,
        &context.token_vault_1,
        &context.token_program,
        amount_1,
    )?;
    emit!(LiquidityChangeEvent {
        pool_state: pool_state.key(),
        tick: pool_state.tick_current,
        tick_lower: tick_lower_index,
        tick_upper: tick_upper_index,
        liquidity_before: liquidity_before,
        liquidity_after: pool_state.liquidity,
    });
    Ok((amount_0, amount_1))
}

pub fn modify_position(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
    timestamp: u64,
) -> Result<(i64, i64, bool, bool)> {
    let (flip_tick_lower, flip_tick_upper) = update_position(
        liquidity_delta,
        pool_state,
        protocol_position_state,
        tick_lower_state,
        tick_upper_state,
        timestamp,
    )?;
    let mut amount_0 = 0;
    let mut amount_1 = 0;

    if liquidity_delta != 0 {
        (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
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
pub fn update_position(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    tick_lower_state: &mut TickState,
    tick_upper_state: &mut TickState,
    timestamp: u64,
) -> Result<(bool, bool)> {
    let updated_reward_infos = pool_state.update_reward_infos(timestamp)?;

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
            &updated_reward_infos,
        )?;
        flipped_upper = tick_upper_state.update(
            pool_state.tick_current,
            liquidity_delta,
            pool_state.fee_growth_global_0_x64,
            pool_state.fee_growth_global_1_x64,
            true,
            &updated_reward_infos,
        )?;
        #[cfg(feature = "enable-log")]
        msg!(
            "tick_upper.reward_growths_outside_x64:{:?}, tick_lower.reward_growths_outside_x64:{:?}",
            identity(tick_upper_state.reward_growths_outside_x64),
            identity(tick_lower_state.reward_growths_outside_x64)
        );
    }

    // Update fees
    let (fee_growth_inside_0_x64, fee_growth_inside_1_x64) = tick_array::get_fee_growth_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        pool_state.fee_growth_global_0_x64,
        pool_state.fee_growth_global_1_x64,
    );

    // Update reward outside if needed
    let reward_growths_inside = tick_array::get_reward_growths_inside(
        tick_lower_state.deref(),
        tick_upper_state.deref(),
        pool_state.tick_current,
        &updated_reward_infos,
    );

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

const METADATA_URI: &str =
    "https://cloudflare-ipfs.com/ipfs/Qmefod1DZcmCCyBdPgNQog2fAbjkNWhwXKEA4ias71pXPX/";
fn get_uri_with_random() -> String {
    let current_timestamp = u64::try_from(Clock::get().unwrap().unix_timestamp).unwrap();
    let current_slot = Clock::get().unwrap().slot;
    // 01 ~ 08
    let random_num = (current_timestamp + current_slot) % 8 + 1;
    // https://cloudflare-ipfs.com/ipfs/Qmefod1DZcmCCyBdPgNQog2fAbjkNWhwXKEA4ias71pXPX/01.json
    let random_str = METADATA_URI.to_string() + "0" + &random_num.to_string() + ".json";
    random_str
}
fn create_nft_with_metadata<'info>(
    payer: &AccountInfo<'info>,
    pool_state_loader: &AccountLoader<'info, PoolState>,
    position_nft_mint: &AccountInfo<'info>,
    position_nft_account: &AccountInfo<'info>,
    metadata_account: &AccountInfo<'info>,
    metadata_program: &AccountInfo<'info>,
    token_program: AccountInfo<'info>,
    system_program: AccountInfo<'info>,
    rent: AccountInfo<'info>,
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    let seeds = [
        &POOL_SEED.as_bytes(),
        pool_state.amm_config.as_ref(),
        pool_state.token_mint_0.as_ref(),
        pool_state.token_mint_1.as_ref(),
        &[pool_state.bump] as &[u8],
    ];
    // Mint the NFT
    token::mint_to(
        CpiContext::new_with_signer(
            token_program.clone(),
            token::MintTo {
                mint: position_nft_mint.clone(),
                to: position_nft_account.clone(),
                authority: pool_state_loader.to_account_info(),
            },
            &[&seeds[..]],
        ),
        1,
    )?;
    let create_metadata_ix = create_metadata_accounts_v2(
        metadata_program.key(),
        metadata_account.key(),
        position_nft_mint.key(),
        pool_state_loader.key(),
        payer.key(),
        pool_state_loader.key(),
        String::from("Raydium Concentrated Liquidity"),
        String::from("RCL"),
        get_uri_with_random(),
        Some(vec![Creator {
            address: pool_state_loader.key(),
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
            pool_state_loader.to_account_info(),
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
                current_authority: pool_state_loader.to_account_info(),
                account_or_mint: position_nft_mint.clone(),
            },
            &[&seeds[..]],
        ),
        AuthorityType::MintTokens,
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod modify_position_test {
    use super::modify_position;
    use crate::error::ErrorCode;
    use crate::libraries::tick_math;
    use crate::states::oracle::block_timestamp_mock;
    use crate::states::pool_test::build_pool;
    use crate::states::protocol_position::*;
    use crate::states::tick_array_test::build_tick;

    #[test]
    fn liquidity_delta_zero_empty_liquidity_not_allowed_test() {
        let pool_state_ref = build_pool(1, 10, 1000, 10000);
        let pool_state = &mut pool_state_ref.borrow_mut();
        let tick_lower_state = &mut build_tick(1, 10, 10).take();
        let tick_upper_state = &mut build_tick(2, 10, -10).take();

        let result = modify_position(
            0,
            pool_state,
            &mut ProtocolPositionState::default(),
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ErrorCode::InvaildLiquidity.into());
    }

    #[test]
    fn init_position_in_range_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = 0;
        let tick_upper_index = 2;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int != 0);
        assert!(amount_1_int != 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity + (liquidity_delta as u128));

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == 0);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - tick_lower_state.fee_growth_outside_0_x64
            - tick_upper_state.fee_growth_outside_0_x64;
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - tick_lower_state.fee_growth_outside_1_x64
            - tick_upper_state.fee_growth_outside_1_x64;
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);

        // check protocol position state
    }

    #[test]
    fn init_position_in_left_of_current_tick_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = -1;
        let tick_upper_index = 0;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int == 0);
        assert!(amount_1_int != 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity_delta as u128);

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == pool_state.fee_growth_global_0_x64);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == pool_state.fee_growth_global_1_x64);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - tick_lower_state.fee_growth_outside_0_x64
            - (pool_state.fee_growth_global_0_x64 - tick_upper_state.fee_growth_outside_0_x64);
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - tick_lower_state.fee_growth_outside_1_x64
            - (pool_state.fee_growth_global_1_x64 - tick_upper_state.fee_growth_outside_1_x64);
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);
    }

    #[test]
    fn init_position_in_right_of_current_tick_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current,
            10,
            tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let tick_lower_index = 2;
        let tick_upper_index = 3;
        let tick_lower_state = &mut build_tick(tick_lower_index, 0, 0).take();
        let tick_upper_state = &mut build_tick(tick_upper_index, 0, 0).take();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int, flip_tick_lower, flip_tick_upper) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            tick_lower_state,
            tick_upper_state,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int != 0);
        assert!(amount_1_int == 0);
        assert_eq!(flip_tick_lower, true);
        assert_eq!(flip_tick_upper, true);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity_delta as u128);

        // check tick state
        assert!(tick_lower_state.is_initialized());
        assert!(tick_lower_state.liquidity_gross == 10000);
        assert!(tick_upper_state.liquidity_gross == 10000);

        assert!(tick_lower_state.liquidity_net == 10000);
        assert!(tick_upper_state.liquidity_net == -10000);

        assert!(tick_lower_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_lower_state.fee_growth_outside_1_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_0_x64 == 0);
        assert!(tick_upper_state.fee_growth_outside_1_x64 == 0);

        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64
            - (pool_state.fee_growth_global_0_x64 - tick_lower_state.fee_growth_outside_0_x64)
            - tick_upper_state.fee_growth_outside_0_x64;
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64
            - (pool_state.fee_growth_global_1_x64 - tick_lower_state.fee_growth_outside_1_x64)
            - tick_upper_state.fee_growth_outside_1_x64;
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);
        assert!(protocol_position.tick_lower_index == tick_lower_index);
        assert!(protocol_position.tick_upper_index == tick_upper_index);

        // check protocol position state
    }
}
