use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_user_to_pool_vault;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

#[derive(Accounts)]
// #[instruction(reward_index: u8)]
pub struct InitializeReward<'info> {
    /// the
    #[account(mut)]
    pub reward_funder: Signer<'info>,
    #[account(
        mut,
        token::mint = reward_token_mint
    )]
    pub funder_token_account: Box<Account<'info, TokenAccount>>,
    /// Set reward for this pool
    #[account(mut)]
    pub pool_state: Box<Account<'info, PoolState>>,
    /// Reward mint
    pub reward_token_mint: Box<Account<'info, Mint>>,
    /// A pda, reward vault
    #[account(
        init,
        seeds =[
            POOL_REWARD_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            reward_token_mint.key().as_ref(),
        ],
        bump,
        payer = reward_funder,
        token::mint = reward_token_mint,
        token::authority = pool_state
    )]
    pub reward_token_vault: Box<Account<'info, TokenAccount>>,
    #[account(address = token::ID)]
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Debug, PartialEq)]
pub struct InitializeRewardParam {
    /// Reward index
    pub reward_index: u8,
    /// Reward open time
    pub open_time: u64,
    /// Reward end time
    pub end_time: u64,
    /// Token reward per second are earned per unit of liquidity
    pub emissions_per_second_x32: u64,
    /// Initialize amount desposit to reward vault account
    pub amount: u64,
}

impl InitializeRewardParam {
    pub fn check(&self, curr_timestamp: u64) -> Result<()> {
        if self.open_time >= self.end_time
            || self.end_time < curr_timestamp
            || self.emissions_per_second_x32 == 0
            || self.reward_index >= NUM_REWARDS as u8
        {
            return Err(ErrorCode::InvalidRewardInitParam.into());
        }
        Ok(())
    }
}

pub fn initialize_reward(
    ctx: Context<InitializeReward>,
    param: InitializeRewardParam,
) -> Result<()> {
    // Clock
    let clock = Clock::get()?;
    param.check(clock.unix_timestamp as u64)?;
    let pool_state = &mut ctx.accounts.pool_state;
    pool_state.initialize_reward(
        clock.unix_timestamp as u64,
        param.reward_index as usize,
        param.open_time,
        param.end_time,
        param.emissions_per_second_x32,
        &ctx.accounts.reward_token_mint.key(),
        &ctx.accounts.reward_token_vault.key(),
    )?;

    if param.amount > 0 {
        transfer_from_user_to_pool_vault(
            &ctx.accounts.reward_funder,
            &ctx.accounts.funder_token_account,
            &ctx.accounts.reward_token_vault,
            &ctx.accounts.token_program,
            param.amount,
        )?;
    }

    Ok(())
}
