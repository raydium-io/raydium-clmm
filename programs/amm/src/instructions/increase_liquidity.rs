use super::{add_liquidity, AddLiquidityParam};
use crate::error::ErrorCode;
use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};

#[derive(Accounts)]
pub struct IncreaseLiquidity<'info> {
    /// Pays to mint the position
    pub nft_owner: Signer<'info>,

    /// The token account for nft
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

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

    /// Increase liquidity for this position
    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Stores init state for the lower tick
    #[account(mut, constraint = tick_array_lower.load()?.pool_id == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    #[account(mut, constraint = tick_array_upper.load()?.pool_id == pool_state.key())]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    /// The payer's token account for token_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
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

    /// Program to create mint account and mint tokens
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

#[derive(Accounts)]
pub struct IncreaseLiquidityV2<'info> {
    /// Pays to mint the position
    pub nft_owner: Signer<'info>,

    /// The token account for nft
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        token::token_program = token_program,
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

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

    /// Increase liquidity for this position
    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Stores init state for the lower tick
    #[account(mut, constraint = tick_array_lower.load()?.pool_id == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    #[account(mut, constraint = tick_array_upper.load()?.pool_id == pool_state.key())]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    /// The payer's token account for token_0
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
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

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,

    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,

    /// The mint of token vault 0
    #[account(
            address = token_vault_0.mint
    )]
    pub vault_0_mint: InterfaceAccount<'info, Mint>,

    /// The mint of token vault 1
    #[account(
            address = token_vault_1.mint
    )]
    pub vault_1_mint: InterfaceAccount<'info, Mint>,
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

pub struct IncreaseLiquidityParam<'b, 'info> {
    /// Pays to mint the position
    pub nft_owner: &'b Signer<'info>,

    /// The token account for nft
    pub nft_account: &'b mut Box<InterfaceAccount<'info, TokenAccount>>,

    pub pool_state: &'b mut AccountLoader<'info, PoolState>,

    pub protocol_position: &'b mut Box<Account<'info, ProtocolPositionState>>,

    /// Increase liquidity for this position
    pub personal_position: &'b mut Box<Account<'info, PersonalPositionState>>,

    /// Stores init state for the lower tick
    pub tick_array_lower: &'b mut AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    pub tick_array_upper: &'b mut AccountLoader<'info, TickArrayState>,

    /// The payer's token account for token_0
    pub token_account_0: &'b mut Box<InterfaceAccount<'info, TokenAccount>>,

    /// The token account spending token_1 to mint the position
    pub token_account_1: &'b mut Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    pub token_vault_0: &'b mut Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    pub token_vault_1: &'b mut Box<InterfaceAccount<'info, TokenAccount>>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,

    /// Token program 2022
    pub token_program_2022: Option<Program<'info, Token2022>>,

    /// The mint of token vault 0
    pub vault_0_mint: Option<InterfaceAccount<'info, Mint>>,
    /// The mint of token vault 1
    pub vault_1_mint: Option<InterfaceAccount<'info, Mint>>,
}

pub fn increase_liquidity<'a, 'b, 'c, 'info>(
    accounts: &mut IncreaseLiquidityParam<'b, 'info>,
    remaining_accounts: &'b [AccountInfo<'info>],
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    base_flag: Option<bool>,
) -> Result<()> {
    let mut pool_state = accounts.pool_state.load_mut()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
        return err!(ErrorCode::NotApproved);
    }
    let tick_lower = accounts.personal_position.tick_lower_index;
    let tick_upper = accounts.personal_position.tick_upper_index;

    let use_tickarray_bitmap_extension =
        pool_state.is_overflow_default_tickarray_bitmap(vec![tick_lower, tick_upper]);
    let mut add_liquidity_context = AddLiquidityParam {
        payer: &accounts.nft_owner,
        token_account_0: accounts.token_account_0,
        token_account_1: accounts.token_account_1,
        token_vault_0: accounts.token_vault_0,
        token_vault_1: accounts.token_vault_1,
        vault_0_mint: accounts.vault_0_mint.clone(),
        vault_1_mint: accounts.vault_1_mint.clone(),
        tick_array_lower: &accounts.tick_array_lower,
        tick_array_upper: &accounts.tick_array_upper,
        protocol_position: accounts.protocol_position,
        token_program: accounts.token_program.clone(),
        token_program_2022: accounts.token_program_2022.clone(),
        tick_array_bitmap_extension: if use_tickarray_bitmap_extension {
            Some(&remaining_accounts[0])
        } else {
            None
        },
    };

    let (amount_0, amount_1, amount_0_transfer_fee, amount_1_transfer_fee) = add_liquidity(
        &mut add_liquidity_context,
        &mut pool_state,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower,
        tick_upper,
        base_flag,
    )?;
    let updated_protocol_position = add_liquidity_context.protocol_position;

    let personal_position = &mut accounts.personal_position;
    personal_position.token_fees_owed_0 = calculate_latest_token_fees(
        personal_position.token_fees_owed_0,
        personal_position.fee_growth_inside_0_last_x64,
        updated_protocol_position.fee_growth_inside_0_last_x64,
        personal_position.liquidity,
    );
    personal_position.token_fees_owed_1 = calculate_latest_token_fees(
        personal_position.token_fees_owed_1,
        personal_position.fee_growth_inside_1_last_x64,
        updated_protocol_position.fee_growth_inside_1_last_x64,
        personal_position.liquidity,
    );

    personal_position.fee_growth_inside_0_last_x64 =
        updated_protocol_position.fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last_x64 =
        updated_protocol_position.fee_growth_inside_1_last_x64;

    // update rewards, must update before increase liquidity
    personal_position.update_rewards(updated_protocol_position.reward_growth_inside, true)?;
    personal_position.liquidity = personal_position.liquidity.checked_add(liquidity).unwrap();

    emit!(IncreaseLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        amount_0,
        amount_1,
        amount_0_transfer_fee,
        amount_1_transfer_fee
    });

    Ok(())
}

pub fn calculate_latest_token_fees(
    last_total_fees: u64,
    fee_growth_inside_last_x64: u128,
    fee_growth_inside_latest_x64: u128,
    liquidity: u128,
) -> u64 {
    let fee_growth_delta =
        U128::from(fee_growth_inside_latest_x64.wrapping_sub(fee_growth_inside_last_x64))
            .mul_div_floor(U128::from(liquidity), U128::from(fixed_point_64::Q64))
            .unwrap()
            .to_underflow_u64();
    #[cfg(feature = "enable-log")]
    msg!("calculate_latest_token_fees fee_growth_delta:{}, fee_growth_inside_latest_x64:{}, fee_growth_inside_last_x64:{}, liquidity:{}", fee_growth_delta, fee_growth_inside_latest_x64, fee_growth_inside_last_x64, liquidity);
    last_total_fees.checked_add(fee_growth_delta).unwrap()
}
