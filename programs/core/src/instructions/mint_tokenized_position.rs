use super::{mint, MintContext};
use crate::error::ErrorCode;
use crate::libraries::{liquidity_amounts, tick_math};
use crate::program::AmmCore;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token;
use anchor_spl::token::{Mint, Token, TokenAccount};
use std::collections::BTreeMap;
use std::mem::size_of;

#[derive(Accounts)]
pub struct MintTokenizedPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub minter: Signer<'info>,

    /// Receives the position NFT
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub recipient: UncheckedAccount<'info>,

    /// The program account acting as the core liquidity custodian for token holder, and as
    /// mint authority of the position NFT
    pub factory_state: AccountLoader<'info, FactoryState>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = factory_state,
        payer = minter
    )]
    pub nft_mint: Box<Account<'info, Mint>>,

    /// Token account where position NFT will be minted
    #[account(
        init,
        associated_token::mint = nft_mint,
        associated_token::authority = recipient,
        payer = minter
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Mint liquidity for this pool
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// Core program account to store position data
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub core_position_state: UncheckedAccount<'info>,

    /// Account to store data for the position's lower tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_lower_state: UncheckedAccount<'info>,

    /// Account to store data for the position's upper tick
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub tick_upper_state: UncheckedAccount<'info>,

    /// Account to mark the lower tick as initialized
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_lower_state: UncheckedAccount<'info>, // remove

    /// Account to mark the upper tick as initialized
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub bitmap_upper_state: UncheckedAccount<'info>, // remove

    /// Metadata for the tokenized position
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), nft_mint.key().as_ref()],
        bump,
        payer = minter,
        space = 8 + size_of::<TokenizedPositionState>()
    )]
    pub tokenized_position_state: AccountLoader<'info, TokenizedPositionState>,

    /// The token account spending token_0 to mint the position
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub token_account_0: UncheckedAccount<'info>,

    /// The token account spending token_1 to mint the position
    /// CHECK: Account validation is performed by the token program
    #[account(mut)]
    pub token_account_1: UncheckedAccount<'info>,

    /// The token account owned by core to hold pool tokens for token_0
    #[account(mut)]
    pub vault_0: Box<Account<'info, TokenAccount>>,

    /// The token account owned by core to hold pool tokens for token_1
    #[account(mut)]
    pub vault_1: Box<Account<'info, TokenAccount>>,

    /// The latest observation state
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub last_observation_state: UncheckedAccount<'info>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// The core program where liquidity is minted
    pub core_program: Program<'info, AmmCore>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,

    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn mint_tokenized_position<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, MintTokenizedPosition<'info>>,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    deadline: i64,
) -> Result<()> {
    // Validate addresses manually, as constraint checks are not applied to internal calls
    let pool_state =
        AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
    let tick_lower_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
    let tick_lower = tick_lower_state.load()?.tick;
    let tick_upper_state =
        AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
    let tick_upper = tick_upper_state.load()?.tick;

    let mut accs = MintContext {
        minter: ctx.accounts.minter.clone(),
        token_account_0: ctx.accounts.token_account_0.clone(),
        token_account_1: ctx.accounts.token_account_1.clone(),
        vault_0: ctx.accounts.vault_0.clone(),
        vault_1: ctx.accounts.vault_1.clone(),
        recipient: UncheckedAccount::try_from(ctx.accounts.factory_state.to_account_info()),
        pool_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower_state: ctx.accounts.bitmap_lower_state.clone(),
        bitmap_upper_state: ctx.accounts.bitmap_upper_state.clone(),
        position_state: ctx.accounts.core_position_state.clone(),
        last_observation_state: ctx.accounts.last_observation_state.clone(),
        token_program: ctx.accounts.token_program.clone(),
        callback_handler: UncheckedAccount::try_from(ctx.accounts.core_program.to_account_info()),
    };

    let (liquidity, amount_0, amount_1) = add_liquidity(
        &mut accs,
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
                mint: ctx.accounts.nft_mint.to_account_info().clone(),
                to: ctx.accounts.nft_account.to_account_info().clone(),
                authority: ctx.accounts.factory_state.to_account_info().clone(),
            },
            &[&[&[ctx.accounts.factory_state.load()?.bump] as &[u8]]],
        ),
        1,
    )?;

    // Write tokenized position metadata
    let mut tokenized_position = ctx.accounts.tokenized_position_state.load_init()?;
    tokenized_position.bump = *ctx.bumps.get("tokenized_position_state").unwrap();
    tokenized_position.mint = ctx.accounts.nft_mint.key();
    tokenized_position.pool_id = ctx.accounts.pool_state.key();

    tokenized_position.tick_lower = tick_lower; // can read from core position
    tokenized_position.tick_upper = tick_upper;
    tokenized_position.liquidity = liquidity;
    tokenized_position.fee_growth_inside_0_last_x32 = AccountLoader::<PositionState>::try_from(
        &ctx.accounts.core_position_state.to_account_info(),
    )?
    .load()?
    .fee_growth_inside_0_last_x32;
    tokenized_position.fee_growth_inside_1_last_x32 = AccountLoader::<PositionState>::try_from(
        &ctx.accounts.core_position_state.to_account_info(),
    )?
    .load()?
    .fee_growth_inside_1_last_x32;

    emit!(IncreaseLiquidityEvent {
        token_id: ctx.accounts.nft_mint.key(),
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
pub fn add_liquidity<'info>(
    accounts: &mut MintContext<'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<(u64, u64, u64)> {
    let sqrt_price_x32 = accounts.pool_state.load()?.sqrt_price_x32;

    let sqrt_ratio_a_x32 = tick_math::get_sqrt_ratio_at_tick(tick_lower)?;
    let sqrt_ratio_b_x32 = tick_math::get_sqrt_ratio_at_tick(tick_upper)?;
    let liquidity = liquidity_amounts::get_liquidity_for_amounts(
        sqrt_price_x32,
        sqrt_ratio_a_x32,
        sqrt_ratio_b_x32,
        amount_0_desired,
        amount_1_desired,
    );

    let balance_0_before = accounts.vault_0.amount;
    let balance_1_before = accounts.vault_1.amount;

    mint(
        Context::new(
            &crate::id(),
            accounts,
            remaining_accounts,
            BTreeMap::default(),
        ),
        liquidity,
    )?;

    accounts.vault_0.reload()?;
    accounts.vault_1.reload()?;
    let amount_0 = accounts.vault_0.amount - balance_0_before;
    let amount_1 = accounts.vault_1.amount - balance_1_before;
    require!(
        amount_0 >= amount_0_min && amount_1 >= amount_1_min,
        ErrorCode::PriceSlippageCheck
    );

    Ok((liquidity, amount_0, amount_1))
}
