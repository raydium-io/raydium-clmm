use crate::error::ErrorCode;
use crate::libraries::{fixed_point_64, full_math::MulDiv, U256};
use crate::states::*;
use crate::util::transfer_from_user_to_pool_vault;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

#[derive(Accounts)]
pub struct InitializeReward<'info> {
    /// The founder deposit reward token to vault
    #[account(mut)]
    pub reward_funder: Signer<'info>,

    // The funder's reward token account
    #[account(
        mut,
        token::mint = reward_token_mint
    )]
    pub funder_token_account: Box<Account<'info, TokenAccount>>,

    /// For check the reward_funder authority
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// Set reward for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

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
    /// Reward open time
    pub open_time: u64,
    /// Reward end time
    pub end_time: u64,
    /// Token reward per second are earned per unit of liquidity
    pub emissions_per_second_x64: u128,
}

impl InitializeRewardParam {
    pub fn check(&self, curr_timestamp: u64) -> Result<()> {
        if self.open_time >= self.end_time
            || self.open_time < curr_timestamp
            || self.end_time < curr_timestamp
            || self.emissions_per_second_x64 == 0
        {
            return Err(ErrorCode::InvalidRewardInitParam.into());
        }
        let time_delta = self.end_time.checked_sub(self.open_time).unwrap();
        if time_delta < reward_period_limit::MIN_REWARD_PERIOD
            || time_delta > reward_period_limit::MAX_REWARD_PERIOD
        {
            return Err(ErrorCode::InvalidRewardPeriod.into());
        }
        Ok(())
    }
}

pub fn initialize_reward(
    ctx: Context<InitializeReward>,
    param: InitializeRewardParam,
) -> Result<()> {
    require!(
        ctx.accounts.reward_funder.key() == ctx.accounts.amm_config.owner
            || ctx.accounts.reward_funder.key() == crate::admin::id(),
        ErrorCode::NotApproved
    );

    // Clock
    let clock = Clock::get()?;
    #[cfg(feature = "enable-log")]
    msg!("current block timestamp:{}", clock.unix_timestamp);
    param.check(clock.unix_timestamp as u64)?;

    let reward_amount = U256::from(param.end_time - param.open_time)
        .mul_div_ceil(
            U256::from(param.emissions_per_second_x64),
            U256::from(fixed_point_64::Q64),
        )
        .unwrap()
        .as_u64();

    require_gte!(ctx.accounts.funder_token_account.amount, reward_amount);

    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    pool_state.initialize_reward(
        param.open_time,
        param.end_time,
        param.emissions_per_second_x64,
        &ctx.accounts.reward_token_mint.key(),
        &ctx.accounts.reward_token_vault.key(),
        &ctx.accounts.reward_funder.key(),
        &ctx.accounts.amm_config.owner,
    )?;

    transfer_from_user_to_pool_vault(
        &ctx.accounts.reward_funder,
        &ctx.accounts.funder_token_account,
        &ctx.accounts.reward_token_vault,
        &ctx.accounts.token_program,
        reward_amount,
    )?;

    Ok(())
}
