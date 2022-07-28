use super::{add_liquidity, MintParam};
use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

#[derive(Accounts)]
pub struct IncreaseLiquidity<'info> {
    /// Pays to mint the position
    pub nft_owner: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position.nft_mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// Authority PDA for the NFT mint
    #[account(address = pool_state.amm_config)]
    pub amm_config: Account<'info, AmmConfig>,

    /// Mint liquidity for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// Core program account to store position data
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

    /// Increase liquidity for this position
    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// Stores init state for the lower tick
    #[account(mut, constraint = tick_array_lower.load()?.amm_pool == pool_state.key())]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    /// Stores init state for the upper tick
    #[account(mut, constraint = tick_array_upper.load()?.amm_pool == pool_state.key())]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    /// The payer's token account for token_0
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

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
}

pub fn increase_liquidity<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    let tick_lower = ctx.accounts.personal_position.tick_lower_index;
    let tick_upper = ctx.accounts.personal_position.tick_upper_index;
    let mut accounts = MintParam {
        payer: &ctx.accounts.nft_owner,
        token_account_0: ctx.accounts.token_account_0.as_mut(),
        token_account_1: ctx.accounts.token_account_1.as_mut(),
        token_vault_0: ctx.accounts.token_vault_0.as_mut(),
        token_vault_1: ctx.accounts.token_vault_1.as_mut(),
        pool_state: ctx.accounts.pool_state.as_mut(),
        tick_array_lower: &ctx.accounts.tick_array_lower,
        tick_array_upper: &ctx.accounts.tick_array_upper,
        protocol_position: ctx.accounts.protocol_position.as_mut(),
        token_program: ctx.accounts.token_program.clone(),
    };
    let (liquidity, amount_0, amount_1) = add_liquidity(
        &mut accounts,
        amount_0_desired,
        amount_1_desired,
        amount_0_min,
        amount_1_min,
        tick_lower,
        tick_upper,
    )?;
    let updated_procotol_position = accounts.protocol_position;
    let fee_growth_inside_0_last_x64 = updated_procotol_position.fee_growth_inside_0_last;
    let fee_growth_inside_1_last_x64 = updated_procotol_position.fee_growth_inside_1_last;

    // Update tokenized position metadata
    let personal_position = ctx.accounts.personal_position.as_mut();
    personal_position.token_fees_owed_0 = personal_position
        .token_fees_owed_0
        .checked_add(
            U128::from(
                fee_growth_inside_0_last_x64
                    .saturating_sub(personal_position.fee_growth_inside_0_last_x64),
            )
            .mul_div_floor(
                U128::from(personal_position.liquidity),
                U128::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64(),
        )
        .unwrap();

    personal_position.token_fees_owed_1 = personal_position
        .token_fees_owed_1
        .checked_add(
            U128::from(
                fee_growth_inside_1_last_x64
                    .saturating_sub(personal_position.fee_growth_inside_1_last_x64),
            )
            .mul_div_floor(
                U128::from(personal_position.liquidity),
                U128::from(fixed_point_64::Q64),
            )
            .unwrap()
            .as_u64(),
        )
        .unwrap();

    personal_position.fee_growth_inside_0_last_x64 = fee_growth_inside_0_last_x64;
    personal_position.fee_growth_inside_1_last_x64 = fee_growth_inside_1_last_x64;

    // update rewards, must update before increase liquidity
    personal_position.update_rewards(updated_procotol_position.reward_growth_inside)?;
    personal_position.liquidity = personal_position.liquidity.checked_add(liquidity).unwrap();

    emit!(ChangeLiquidityEvent {
        position_nft_mint: personal_position.nft_mint,
        liquidity,
        amount_0,
        amount_1
    });

    Ok(())
}
