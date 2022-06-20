use super::{burn, BurnParam};
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use std::ops::Deref;

#[derive(Accounts)]
pub struct UpdateFeeAndRewards<'info> {
    /// The position owner or delegated authority
    pub owner_or_delegate: Signer<'info>,

    /// The token account for the tokenized position
    #[account(
        constraint = nft_account.mint == personal_position_state.mint
    )]
    pub nft_account: Box<Account<'info, TokenAccount>>,

    /// The program account of the NFT for which tokens are being collected
    #[account(mut)]
    pub personal_position_state: Box<Account<'info, PersonalPositionState>>,

    /// The program account acting as the core liquidity custodian for token holder
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account for the liquidity pool from which fees are collected
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,

    /// The program account to access the core program position state
    #[account(mut)]
    pub protocol_position_state: Box<Account<'info, ProcotolPositionState>>,

    /// The program account for the position's lower tick
    #[account(mut)]
    pub tick_lower_state: Box<Account<'info, TickState>>,

    /// The program account for the position's upper tick
    #[account(mut)]
    pub tick_upper_state: Box<Account<'info, TickState>>,

    /// The bitmap program account for the init state of the lower tick
    #[account(mut)]
    pub bitmap_lower_state: AccountLoader<'info, TickBitmapState>,

    /// Stores init state for the upper tick
    #[account(mut)]
    pub bitmap_upper_state: AccountLoader<'info, TickBitmapState>,

    /// The latest observation state
    #[account(mut)]
    pub last_observation_state: Box<Account<'info, ObservationState>>,
}

pub fn update_fee_and_rewards<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, UpdateFeeAndRewards<'info>>,
) -> Result<()> {
    let tokenized_position = ctx.accounts.personal_position_state.as_mut();

    let mut protocol_position_owner = ctx.accounts.amm_config.to_account_info();
    protocol_position_owner.is_signer = true;
    // trigger an update of the position fees owed and fee growth snapshots if it has any liquidity
    if tokenized_position.liquidity > 0 {
        let mut burn_accounts = BurnParam {
            owner: &Signer::try_from(&protocol_position_owner)?,
            pool_state: ctx.accounts.pool_state.as_mut(),
            tick_lower_state: ctx.accounts.tick_lower_state.as_mut(),
            tick_upper_state: ctx.accounts.tick_upper_state.as_mut(),
            bitmap_lower_state: &ctx.accounts.bitmap_lower_state,
            bitmap_upper_state: &ctx.accounts.bitmap_upper_state,
            position_state: ctx.accounts.protocol_position_state.as_mut(),
            last_observation_state: ctx.accounts.last_observation_state.as_mut(),
        };
        // update fee and reward inside
        burn(&mut burn_accounts, ctx.remaining_accounts, 0)?;

        let updated_core_position = burn_accounts.position_state.deref();

        tokenized_position.tokens_owed_0 = tokenized_position
            .tokens_owed_0
            .checked_add(
                (updated_core_position.fee_growth_inside_0_last
                    - tokenized_position.fee_growth_inside_0_last)
                    .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                    .unwrap(),
            )
            .unwrap();
        tokenized_position.tokens_owed_1 = tokenized_position
            .tokens_owed_1
            .checked_add(
                (updated_core_position.fee_growth_inside_1_last
                    - tokenized_position.fee_growth_inside_1_last)
                    .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                    .unwrap(),
            )
            .unwrap();

        tokenized_position.fee_growth_inside_0_last =
            updated_core_position.fee_growth_inside_0_last;
        tokenized_position.fee_growth_inside_1_last =
            updated_core_position.fee_growth_inside_1_last;

        tokenized_position.update_rewards(updated_core_position.reward_growth_inside)?;
    }

    emit!(UpdateFeeAndRewardsEvent {
        position_nft_mint: tokenized_position.mint,
        tokens_owed_0: tokenized_position.tokens_owed_0,
        tokens_owed_1: tokenized_position.tokens_owed_1,
        reward_infos: tokenized_position.reward_infos
    });

    Ok(())
}
