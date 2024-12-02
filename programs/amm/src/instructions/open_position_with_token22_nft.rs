use super::open_position::open_position;
use crate::states::*;
use crate::util::create_position_nft_mint_with_extensions;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::{create, AssociatedToken, Create};
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct OpenPositionWithToken22Nft<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address, initialize in constract
    #[account(mut)]
    pub position_nft_mint: Signer<'info>,

    /// CHECK: ATA address where position NFT will be minted, initialize in constract
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

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

    /// CHECK:  Account to store data for the position's lower tick
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

    /// CHECK: Account to store data for the position's upper tick
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
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token_1 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to transfer for token account
    pub token_program: Program<'info, Token>,

    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// Program to create NFT mint/token account and transfer for token22 account
    pub token_program_2022: Program<'info, Token2022>,

    /// The mint of token vault 0
    #[account(
        address = token_vault_0.mint
    )]
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token vault 1
    #[account(
        address = token_vault_1.mint
    )]
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,
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

pub fn open_position_with_token22_nft<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, OpenPositionWithToken22Nft<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    with_metadata: bool,
    base_flag: Option<bool>,
) -> Result<()> {
    create_position_nft_mint_with_extensions(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.personal_position.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program_2022,
        with_metadata,
    )?;

    // create user position nft account
    create(CpiContext::new(
        ctx.accounts.associated_token_program.to_account_info(),
        Create {
            payer: ctx.accounts.payer.to_account_info(),
            associated_token: ctx.accounts.position_nft_account.to_account_info(),
            authority: ctx.accounts.position_nft_owner.to_account_info(),
            mint: ctx.accounts.position_nft_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program_2022.to_account_info(),
        },
    ))?;

    open_position(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_owner,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.position_nft_account,
        None,
        &ctx.accounts.pool_state,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_account_0.to_account_info(),
        &ctx.accounts.token_account_1.to_account_info(),
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.rent,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &ctx.accounts.associated_token_program,
        None,
        Some(&ctx.accounts.token_program_2022),
        Some(ctx.accounts.vault_0_mint.clone()),
        Some(ctx.accounts.vault_1_mint.clone()),
        &ctx.remaining_accounts,
        ctx.bumps.protocol_position,
        ctx.bumps.personal_position,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower_index,
        tick_upper_index,
        tick_array_lower_start_index,
        tick_array_upper_start_index,
        with_metadata,
        base_flag,
        true,
    )
}
