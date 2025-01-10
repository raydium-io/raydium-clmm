use crate::error::ErrorCode;
use crate::states::*;
use crate::util::AccountLoad;
use crate::{libraries::tick_math, util};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

// use solana_program::{program::invoke_signed, system_instruction};
#[derive(Accounts)]
pub struct CreatePool<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub pool_creator: Signer<'info>,

    /// Which config the pool belongs to.
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// CHECK: Initialize an account to store the pool state
    /// seeds = [
    ///     POOL_SEED.as_bytes(),
    ///     amm_config.key().as_ref(),
    ///     token_mint_0.key().as_ref(),
    ///     token_mint_1.key().as_ref(),
    /// ]
    ///  or
    /// if the instruction nonce parameter is passed
    /// seeds = [
    ///     POOL_SEED.as_bytes(),
    ///     amm_config.key().as_ref(),
    ///     token_mint_0.key().as_ref(),
    ///     token_mint_1.key().as_ref(),
    ///     nonce.to_be_bytes()
    /// ],
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

    /// Token_0 mint, the key must be smaller then token_1 mint.
    #[account(
        constraint = token_mint_0.key() < token_mint_1.key(),
        mint::token_program = token_program_0
    )]
    pub token_mint_0: Box<InterfaceAccount<'info, Mint>>,

    /// Token_1 mint
    #[account(
        mint::token_program = token_program_1
    )]
    pub token_mint_1: Box<InterfaceAccount<'info, Mint>>,

    /// Token_0 vault for the pool
    #[account(
        init,
        seeds =[
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_mint_0.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        token::mint = token_mint_0,
        token::authority = pool_state,
        token::token_program = token_program_0,
    )]
    pub token_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token_1 vault for the pool
    #[account(
        init,
        seeds =[
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_mint_1.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        token::mint = token_mint_1,
        token::authority = pool_state,
        token::token_program = token_program_1,
    )]
    pub token_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Initialize an account to store oracle observations
    #[account(
        init,
        seeds = [
            OBSERVATION_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        space = ObservationState::LEN
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// Initialize an account to store if a tick array is initialized.
    #[account(
        init,
        seeds = [
            POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = pool_creator,
        space = TickArrayBitmapExtension::LEN
    )]
    pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,

    /// Spl token program or token program 2022
    pub token_program_0: Interface<'info, TokenInterface>,
    /// Spl token program or token program 2022
    pub token_program_1: Interface<'info, TokenInterface>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_pool(
    ctx: Context<CreatePool>,
    sqrt_price_x64: u128,
    open_time: u64,
    nonce: Option<u8>,
) -> Result<()> {
    if !(util::is_supported_mint(&ctx.accounts.token_mint_0).unwrap()
        && util::is_supported_mint(&ctx.accounts.token_mint_1).unwrap())
    {
        return err!(ErrorCode::NotSupportMint);
    }
    let pool_id = ctx.accounts.pool_state.key();
    let (pool_state_loader, bump) = create_pool_account(
        &ctx.accounts.pool_creator.to_account_info(),
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.amm_config.to_account_info(),
        &ctx.accounts.token_mint_0.to_account_info(),
        &ctx.accounts.token_mint_1.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        nonce,
    )?;
    let pool_state = &mut pool_state_loader.load_init()?;

    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64)?;
    #[cfg(feature = "enable-log")]
    msg!(
        "create pool, init_price: {}, init_tick:{}",
        sqrt_price_x64,
        tick
    );
    // init observation
    ctx.accounts
        .observation_state
        .load_init()?
        .initialize(pool_id)?;

    pool_state.initialize(
        nonce,
        bump,
        sqrt_price_x64,
        open_time,
        tick,
        ctx.accounts.pool_creator.key(),
        ctx.accounts.token_vault_0.key(),
        ctx.accounts.token_vault_1.key(),
        ctx.accounts.amm_config.as_ref(),
        ctx.accounts.token_mint_0.as_ref(),
        ctx.accounts.token_mint_1.as_ref(),
        ctx.accounts.observation_state.key(),
    )?;

    ctx.accounts
        .tick_array_bitmap
        .load_init()?
        .initialize(pool_id);

    emit!(PoolCreatedEvent {
        token_mint_0: ctx.accounts.token_mint_0.key(),
        token_mint_1: ctx.accounts.token_mint_1.key(),
        tick_spacing: ctx.accounts.amm_config.tick_spacing,
        pool_state: ctx.accounts.pool_state.key(),
        sqrt_price_x64,
        tick,
        token_vault_0: ctx.accounts.token_vault_0.key(),
        token_vault_1: ctx.accounts.token_vault_1.key(),
    });
    Ok(())
}

pub fn create_pool_account<'info>(
    payer: &AccountInfo<'info>,
    pool_account_info: &AccountInfo<'info>,
    amm_config: &AccountInfo<'info>,
    token_0_mint: &AccountInfo<'info>,
    token_1_mint: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    nonce: Option<u8>,
) -> Result<(AccountLoad<'info, PoolState>, u8)> {
    if pool_account_info.owner != &system_program::ID {
        return err!(ErrorCode::NotApproved);
    }
    let nonce = if nonce.is_some() {
        require_neq!(nonce.unwrap(), 0);
        vec![nonce.unwrap()]
    } else {
        Vec::new()
    };

    let mut seeds = [
        POOL_SEED.as_bytes(),
        amm_config.key.as_ref(),
        token_0_mint.key.as_ref(),
        token_1_mint.key.as_ref(),
        nonce.as_ref(),
    ]
    .to_vec();

    let (expect_pda_address, bump) = Pubkey::find_program_address(&seeds, &crate::id());
    require_keys_eq!(pool_account_info.key(), expect_pda_address);
    let bump_vec = vec![bump];
    seeds.push(bump_vec.as_ref());
    util::create_or_allocate_account(
        &crate::id(),
        payer.to_account_info(),
        system_program.to_account_info(),
        pool_account_info.clone(),
        &seeds,
        PoolState::LEN,
    )?;

    Ok((
        AccountLoad::<PoolState>::try_from_unchecked(&crate::id(), &pool_account_info)?,
        bump,
    ))
}
