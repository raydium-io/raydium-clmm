pub mod access_control;
pub mod error;
pub mod instructions;
pub mod libraries;
pub mod states;
pub mod util;

use crate::access_control::*;
use anchor_lang::prelude::*;
use instructions::*;
use states::*;

#[cfg(feature = "devnet")]
declare_id!("devKfPVu9CaDvG47KG7bDKexFvAY37Tgp6rPHTruuqU");
#[cfg(not(feature = "devnet"))]
declare_id!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

pub mod admin {
    use anchor_lang::prelude::declare_id;
    #[cfg(feature = "devnet")]
    declare_id!("adMCyoCgfkg7bQiJ9aBJ59H3BXLY3r5LNLfPpQfMzBe");
    #[cfg(not(feature = "devnet"))]
    declare_id!("HggGrUeg4ReGvpPMLJMFKV69NTXL1r4wQ9Pk9Ljutwyv");
}

#[program]
pub mod amm_v3 {

    use super::*;

    // The configuation of AMM protocol, include trade fee and protocol fee
    /// # Arguments
    ///
    /// * `ctx`- The accounts needed by instruction.
    /// * `index` - The index of amm config, there may be multiple config.
    /// * `tick_spacing` - The tickspacing binding with config, cannot be changed.
    /// * `trade_fee_rate` - Trade fee rate, can be changed.
    /// * `protocol_fee_rate` - The rate of protocol fee within tarde fee.
    ///
    pub fn create_amm_config(
        ctx: Context<CreateAmmConfig>,
        index: u16,
        tick_spacing: u16,
        trade_fee_rate: u32,
        protocol_fee_rate: u32,
    ) -> Result<()> {
        assert!(protocol_fee_rate > 0 && protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        assert!(trade_fee_rate < 1_000_000); // 100%
        assert!(tick_spacing > 0 && tick_spacing < 16384);
        instructions::create_amm_config(ctx, index, tick_spacing, protocol_fee_rate, trade_fee_rate)
    }

    /// Updates the owner of the amm config
    /// Must be called by the current owner or admin
    ///
    /// # Arguments
    ///
    /// * `ctx`- Checks whether protocol owner has signed
    ///
    pub fn set_new_owner(ctx: Context<SetNewOwner>) -> Result<()> {
        instructions::set_new_owner(ctx)
    }

    /// Creates a pool for the given token pair and the initial price
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `sqrt_price_x64` - the initial sqrt price (amount_token_1 / amount_token_0) of the pool as a Q64.64
    ///
    pub fn create_pool(ctx: Context<CreatePool>, sqrt_price_x64: u128) -> Result<()> {
        instructions::create_pool(ctx, sqrt_price_x64)
    }

    /// Reset a pool sqrt price, only can be reset if the pool hasn't be used.
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `sqrt_price_x64` - the reset sqrt price of the pool as a Q64.64
    ///
    pub fn reset_sqrt_price(ctx: Context<ResetSqrtPrice>, sqrt_price_x64: u128) -> Result<()> {
        instructions::reset_sqrt_price(ctx, sqrt_price_x64)
    }

    /// Initialize a reward info for a given pool and reward index
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `reward_index` - the index to reward info
    /// * `open_time` - reward open timestamp
    /// * `end_time` - reward end timestamp
    /// * `reward_per_second` - Token reward per second are earned per unit of liquidity.
    ///
    pub fn initialize_reward(
        ctx: Context<InitializeReward>,
        param: InitializeRewardParam,
    ) -> Result<()> {
        instructions::initialize_reward(ctx, param)
    }

    /// Update rewards info of the given pool, can be called for everyone
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    ///
    pub fn update_reward_infos<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, UpdateRewardInfos<'info>>,
    ) -> Result<()> {
        instructions::update_reward_infos(ctx)
    }

    /// Set reward emission per second.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `reward_index` - The index of reward token in the pool.
    /// * `emissions_per_second_x64` - The per second emission reward
    ///
    pub fn set_reward_emissions<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SetRewardEmissions<'info>>,
        reward_index: u8,
        emissions_per_second_x64: u128,
    ) -> Result<()> {
        instructions::set_reward_emissions(ctx, reward_index, emissions_per_second_x64)
    }

    /// Restset the protocol fee rate of the fees.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `protocol_fee_rate` - new protocol fee rate
    ///
    pub fn set_protocol_fee_rate(
        ctx: Context<SetProtocolFeeRate>,
        protocol_fee_rate: u32,
    ) -> Result<()> {
        instructions::set_protocol_fee_rate(ctx, protocol_fee_rate)
    }

    /// Collect the protocol fee accrued to the pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount_0_requested` - The maximum amount of token_0 to send, can be 0 to collect fees in only token_1
    /// * `amount_1_requested` - The maximum amount of token_1 to send, can be 0 to collect fees in only token_0
    ///
    pub fn collect_protocol_fee(
        ctx: Context<CollectProtocolFee>,
        amount_0_requested: u64,
        amount_1_requested: u64,
    ) -> Result<()> {
        instructions::collect_protocol_fee(ctx, amount_0_requested, amount_1_requested)
    }

    /// Creates a new position wrapped in a NFT
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `tick_lower_index` - The low boundary of market
    /// * `tick_upper_index` - The upper boundary of market
    /// * `tick_array_lower_start_index` - The start index of tick array which include tick low
    /// * `tick_array_upper_start_index` - The start index of tick array which include tick upper
    /// * `liquidity` - The liquidity to be added
    /// * `amount_0_min` - The minimum amount of token_0 to spend, which serves as a slippage check
    /// * `amount_1_min` - The minimum amount of token_1 to spend, which serves as a slippage check
    ///
    pub fn open_position<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, OpenPosition<'info>>,
        tick_lower_index: i32,
        tick_upper_index: i32,
        tick_array_lower_start_index: i32,
        tick_array_upper_start_index: i32,
        liquidity: u128,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::open_position(
            ctx,
            liquidity,
            amount_0_min,
            amount_1_min,
            tick_lower_index,
            tick_upper_index,
            tick_array_lower_start_index,
            tick_array_upper_start_index,
        )
    }

    /// Close a position, the nft mint and nft account
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    ///
    pub fn close_position<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ClosePosition<'info>>,
    ) -> Result<()> {
        instructions::close_position(ctx)
    }

    /// Increases liquidity with a exist position, with amount paid by `payer`
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `liquidity` - The desired liquidity to be added
    /// * `amount_0_min` - The minimum amount of token_0 to spend, which serves as a slippage check
    /// * `amount_1_min` - The minimum amount of token_1 to spend, which serves as a slippage check
    ///
    #[access_control(is_authorized_for_token(&ctx.accounts.nft_owner, &ctx.accounts.nft_account))]
    pub fn increase_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
        liquidity: u128,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::increase_liquidity(ctx, liquidity, amount_0_min, amount_1_min)
    }

    /// Decreases liquidity with a exist position
    ///
    /// # Arguments
    ///
    /// * `ctx` -  The context of accounts
    /// * `liquidity` - The amount by which liquidity will be decreased
    /// * `amount_0_min` - The minimum amount of token_0 that should be accounted for the burned liquidity
    /// * `amount_1_min` - The minimum amount of token_1 that should be accounted for the burned liquidity
    ///
    #[access_control(is_authorized_for_token(&ctx.accounts.nft_owner, &ctx.accounts.nft_account))]
    pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
        liquidity: u128,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::decrease_liquidity(ctx, liquidity, amount_0_min, amount_1_min)
    }

    /// Swaps one token for as much as possible of another token across a single pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount` - Arranged in pairs with other_amount_threshold. (amount_in, amount_out_minimum) or (amount_out, amount_in_maximum)
    /// * `other_amount_threshold` - For slippage check
    /// * `sqrt_price_limit` - The Q64.64 sqrt price √P limit. If zero for one, the price cannot
    /// * `is_base_input` - swap base input or swap base output
    ///
    pub fn swap<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit_x64: u128,
        is_base_input: bool,
    ) -> Result<()> {
        instructions::swap(
            ctx,
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64,
            is_base_input,
        )
    }

    /// Swap token for as much as possible of another token across the path provided, base input
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount_in` - Token amount to be swapped in
    /// * `amount_out_minimum` - Panic if output amount is below minimum amount. For slippage.
    ///
    pub fn swap_router_base_in<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapRouterBaseIn<'info>>,
        amount_in: u64,
        amount_out_minimum: u64,
    ) -> Result<()> {
        instructions::swap_router_base_in(ctx, amount_in, amount_out_minimum)
    }
}
