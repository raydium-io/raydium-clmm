pub mod access_control;
pub mod error;
pub mod instructions;
pub mod libraries;
pub mod states;
pub mod util;

use crate::access_control::*;
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use anchor_lang::prelude::*;
use instructions::*;
use states::*;

declare_id!("FA51eLCcZMd2Cm63g9GfY7MBdftKN8aARDtwyqBdyhQp");

#[program]
pub mod amm_core {

    use super::*;

    // ---------------------------------------------------------------------
    // Factory instructions

    // The Factory facilitates creation of pools and control over the protocol fees

    /// Initialize the factory state and set the protocol owner
    ///
    /// # Arguments
    ///
    /// * `ctx`- Initializes the factory state account
    /// * `amm_config_bump` - Bump to validate factory state address
    ///
    pub fn create_amm_config(ctx: Context<CreateAmmConfig>, protocol_fee_rate: u8) -> Result<()> {
        assert!(protocol_fee_rate >= 2 && protocol_fee_rate <= 10);
        instructions::create_amm_config(ctx, protocol_fee_rate)
    }

    /// Updates the owner of the factory
    /// Must be called by the current owner
    ///
    /// # Arguments
    ///
    /// * `ctx`- Checks whether protocol owner has signed
    ///
    pub fn set_new_owner(ctx: Context<SetNewOwner>) -> Result<()> {
        instructions::set_new_owner(ctx)
    }

    /// Create a fee account with the given tick_spacing
    /// Fee account may never be removed once created
    ///
    /// # Arguments
    ///
    /// * `ctx`- Checks whether protocol owner has signed and initializes the fee account
    /// * `fee_state_bump` - Bump to validate fee state address
    /// * `fee` - The fee amount to enable, denominated in hundredths of a bip (i.e. 1e-6)
    /// * `tick_spacing` - The spacing between ticks to be enforced for all pools created
    /// with the given fee amount
    ///
    pub fn create_fee_account(
        ctx: Context<CreateFeeAccount>,
        fee: u32,
        tick_spacing: u16,
    ) -> Result<()> {
        instructions::create_fee_account(ctx, fee, tick_spacing)
    }

    // ---------------------------------------------------------------------
    // Pool instructions

    /// Creates a pool for the given token pair and fee, and sets the initial price
    ///
    /// A single function in place of Uniswap's Factory.createPool(), PoolDeployer.deploy()
    /// Pool.initialize() and pool.Constructor()
    ///
    /// # Arguments
    ///
    /// * `ctx`- Validates token addresses and fee state. Initializes pool, observation and
    /// token accounts
    /// * `pool_state_bump` - Bump to validate Pool State address
    /// * `observation_state_bump` - Bump to validate Observation State address
    /// * `sqrt_price_x32` - the initial sqrt price (amount_token_1 / amount_token_0) of the pool as a Q32.32
    ///
    pub fn create_pool(ctx: Context<CreatePool>, sqrt_price: u64) -> Result<()> {
        instructions::create_pool(ctx, sqrt_price)
    }

    // ---------------------------------------------------------------------
    // Oracle

    /// Increase the maximum number of price and liquidity observations that this pool will store
    ///
    /// An `ObservationState` account is created per unit increase in cardinality_next,
    /// and `observation_cardinality_next` is accordingly incremented.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds the pool and payer addresses, along with a vector of
    /// observation accounts which will be initialized
    /// * `observation_account_bumps` - Vector of bumps to initialize the observation state PDAs
    ///
    pub fn increase_observation_cardinality_next<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, IncreaseObservationCardinalityNextCtx<'info>>,
        observation_account_bumps: Vec<u8>,
    ) -> Result<()> {
        instructions::increase_observation_cardinality_next(ctx, observation_account_bumps)
    }

    // ---------------------------------------------------------------------
    // Pool owner instructions

    /// Set the denominator of the protocol's % share of the fees.
    ///
    /// Unlike Uniswap, protocol fee is globally set. It can be updated by factory owner
    /// at any time.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Checks for valid owner by looking at signer and factory owner addresses.
    /// Holds the Factory State account where protocol fee will be saved.
    /// * `fee_protocol` - new protocol fee for all pools
    ///
    pub fn set_protocol_fee_rate(
        ctx: Context<SetProtocolFeeRate>,
        protocol_fee_rate: u8,
    ) -> Result<()> {
        assert!(protocol_fee_rate >= 2 && protocol_fee_rate <= 10);
        let amm_config = &mut ctx.accounts.amm_config;
        let protocol_fee_rate_old = amm_config.protocol_fee_rate;
        amm_config.protocol_fee_rate = protocol_fee_rate;

        emit!(SetProtocolFeeRateEvent {
            protocol_fee_rate_old,
            protocol_fee_rate
        });

        Ok(())
    }

    /// Collect the protocol fee accrued to the pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - Checks for valid owner by looking at signer and factory owner addresses.
    /// Holds the Pool State account where accrued protocol fee is saved, and token accounts to perform
    /// transfer.
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
    /// ---------------------------------------------------------------------
    /// Account init instructions
    ///
    /// Having separate instructions to initialize instructions saves compute units
    /// and reduces code in downstream instructions
    ///

    /// Initializes an empty program account for a price tick
    ///
    /// # Arguments
    ///
    /// * `ctx` - Contains accounts to initialize an empty tick account
    /// * `tick_account_bump` - Bump to validate tick account PDA
    /// * `tick` - The tick for which the account is created
    ///
    pub fn create_tick_account(ctx: Context<CreateTickAccount>, tick: i32) -> Result<()> {
        let pool_state = &mut ctx.accounts.pool_state;
        check_tick(tick, pool_state.tick_spacing)?;
        let tick_state = &mut ctx.accounts.tick_state;
        tick_state.bump = *ctx.bumps.get("tick_state").unwrap();
        tick_state.tick = tick;
        Ok(())
    }

    /// Initializes an empty program account for a tick bitmap
    ///
    /// # Arguments
    ///
    /// * `ctx` - Contains accounts to initialize an empty bitmap account
    /// * `bitmap_account_bump` - Bump to validate the bitmap account PDA
    /// * `word_pos` - The bitmap key for which to create account. To find word position from a tick,
    /// divide the tick by tick spacing to get a 24 bit compressed result, then right shift to obtain the
    /// most significant 16 bits.
    ///
    pub fn create_bitmap_account(ctx: Context<CreateBitmapAccount>, word_pos: i16) -> Result<()> {
        let max_word_pos =
            ((tick_math::MAX_TICK / ctx.accounts.pool_state.tick_spacing as i32) >> 8) as i16;
        let min_word_pos =
            ((tick_math::MIN_TICK / ctx.accounts.pool_state.tick_spacing as i32) >> 8) as i16;
        require!(word_pos >= min_word_pos, ErrorCode::TickLowerOverflow);
        require!(word_pos <= max_word_pos, ErrorCode::TickUpperOverflow);

        let mut bitmap_account = ctx.accounts.bitmap_state.load_init()?;
        bitmap_account.bump = *ctx.bumps.get("bitmap_state").unwrap();
        bitmap_account.word_pos = word_pos;
        Ok(())
    }

    /// Initializes an empty program account for a position
    ///
    /// # Arguments
    ///
    /// * `ctx` - Contains accounts to initialize an empty position account
    /// * `bump` - Bump to validate the position account PDA
    /// * `tick` - The tick for which the bitmap account is created. Program address of
    /// the account is derived using most significant 16 bits of the tick
    ///
    pub fn create_procotol_position(ctx: Context<CreateProtocolPosition>) -> Result<()> {
        let position_account = &mut ctx.accounts.position_state;
        position_account.bump = *ctx.bumps.get("position_state").unwrap();
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Position instructions

    // Non fungible position manager

    /// Creates a new position wrapped in a NFT
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds pool, tick, bitmap, position and token accounts
    /// * `amount_0_desired` - Desired amount of token_0 to be spent
    /// * `amount_1_desired` - Desired amount of token_1 to be spent
    /// * `amount_0_min` - The minimum amount of token_0 to spend, which serves as a slippage check
    /// * `amount_1_min` - The minimum amount of token_1 to spend, which serves as a slippage check
    /// * `deadline` - The time by which the transaction must be included to effect the change
    ///
    pub fn create_personal_position<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, CreatePersonalPosition<'info>>,
        amount_0_desired: u64,
        amount_1_desired: u64,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::create_personal_position(
            ctx,
            amount_0_desired,
            amount_1_desired,
            amount_0_min,
            amount_1_min,
        )
    }
    /// Attach metaplex metadata to a tokenized position. Permissionless to call.
    /// Optional and cosmetic in nature.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds validated metadata account and tokenized position addresses
    ///
    pub fn personal_position_with_metadata(
        ctx: Context<PersonalPositionWithMetadata>,
    ) -> Result<()> {
        instructions::personal_position_with_metadata(ctx)
    }

    /// Increases liquidity in a tokenized position, with amount paid by `payer`
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds the pool, tick, bitmap, position and token accounts
    /// * `amount_0_desired` - Desired amount of token_0 to be spent
    /// * `amount_1_desired` - Desired amount of token_1 to be spent
    /// * `amount_0_min` - The minimum amount of token_0 to spend, which serves as a slippage check
    /// * `amount_1_min` - The minimum amount of token_1 to spend, which serves as a slippage check
    /// * `deadline` - The time by which the transaction must be included to effect the change
    ///
    pub fn increase_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
        amount_0_desired: u64,
        amount_1_desired: u64,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::increase_liquidity(
            ctx,
            amount_0_desired,
            amount_1_desired,
            amount_0_min,
            amount_1_min,
        )
    }
    /// Decreases the amount of liquidity in a position and accounts it to the position
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds the pool, tick, bitmap, position and token accounts
    /// * `liquidity` - The amount by which liquidity will be decreased
    /// * `amount_0_min` - The minimum amount of token_0 that should be accounted for the burned liquidity
    /// * `amount_1_min` - The minimum amount of token_1 that should be accounted for the burned liquidity
    /// * `deadline` - The time by which the transaction must be included to effect the change
    ///
    #[access_control(is_authorized_for_token(&ctx.accounts.owner_or_delegate, &ctx.accounts.nft_account))]
    pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
        liquidity: u64,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        instructions::decrease_liquidity(ctx, liquidity, amount_0_min, amount_1_min)
    }

    /// Collects up to a maximum amount of fees owed to a specific tokenized position to the recipient
    ///
    /// # Arguments
    ///
    /// * `ctx` - Validated addresses of the tokenized position and token accounts. Fees can be sent
    /// to third parties
    /// * `amount_0_max` - The maximum amount of token0 to collect
    /// * `amount_1_max` - The maximum amount of token0 to collect
    ///
    #[access_control(is_authorized_for_token(&ctx.accounts.owner_or_delegate, &ctx.accounts.nft_account))]
    pub fn collect_fee<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, CollectFee<'info>>,
        amount_0_max: u64,
        amount_1_max: u64,
    ) -> Result<()> {
        instructions::collect_fee(ctx, amount_0_max, amount_1_max)
    }

    /// Swaps `amount_in` of one token for as much as possible of another token,
    /// across a single pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - Accounts required for the swap
    /// * `deadline` - The time by which the transaction must be included to effect the change
    /// * `amount_in` - Arranged in pairs with other_amount_threshold. (amount_in, amount_out_minimum) or (amount_out, amount_in_maximum)
    /// * `other_amount_threshold` - For slippage check
    /// * `sqrt_price_limit` - The Q32.32 sqrt price √P limit. If zero for one, the price cannot
    /// be less than this value after the swap.  If one for zero, the price cannot be greater than
    /// this value after the swap.
    ///
    pub fn swap_single<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit: u64,
        is_base_input: bool,
    ) -> Result<()> {
        instructions::swap_single(
            ctx,
            amount,
            other_amount_threshold,
            sqrt_price_limit,
            is_base_input,
        )
    }
    /// Swaps `amount_in` of one token for as much as possible of another token,
    /// across the path provided
    ///
    /// # Arguments
    ///
    /// * `ctx` - Accounts for token transfer and swap route
    /// * `deadline` - Swap should if fail if past deadline
    /// * `amount_in` - Token amount to be swapped in
    /// * `amount_out_minimum` - Panic if output amount is below minimum amount. For slippage.
    /// * `additional_accounts_per_pool` - Additional observation, bitmap and tick accounts per pool
    ///
    pub fn swap_base_in<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapBaseIn<'info>>,
        amount_in: u64,
        amount_out_minimum: u64,
        additional_accounts_per_pool: Vec<u8>,
    ) -> Result<()> {
        instructions::swap_base_in(
            ctx,
            amount_in,
            amount_out_minimum,
            additional_accounts_per_pool,
        )
    }

    // /// Swaps as little as possible of one token for `amount_out` of another
    // /// along the specified path (reversed)
    // ///
    // /// # Arguments
    // ///
    // /// * `ctx` - Accounts for token transfer and swap route
    // /// * `deadline` - Swap should if fail if past deadline
    // /// * `amount_out` - Token amount to be swapped out
    // /// * `amount_in_maximum` - For slippage. Panic if required input exceeds max limit.
    // ///
    // pub fn exact_output(
    //     ctx: Context<ExactInput>,
    //     deadline: u64,
    //     amount_out: u64,
    //     amount_out_maximum: u64,
    // ) -> Result<()> {
    //     todo!()
    // }
}

/// Common checks for a valid tick input.
/// A tick is valid iff it lies within tick boundaries and it is a multiple
/// of tick spacing.
///
/// # Arguments
///
/// * `tick` - The price tick
///
pub fn check_tick(tick: i32, tick_spacing: u16) -> Result<()> {
    require!(tick >= tick_math::MIN_TICK, ErrorCode::TickLowerOverflow);
    require!(tick <= tick_math::MAX_TICK, ErrorCode::TickUpperOverflow);
    require!(
        tick % tick_spacing as i32 == 0,
        ErrorCode::TickAndSpacingNotMatch
    );
    Ok(())
}

/// Common checks for valid tick inputs.
///
/// # Arguments
///
/// * `tick_lower` - The lower tick
/// * `tick_upper` - The upper tick
///
pub fn check_ticks(tick_lower: i32, tick_upper: i32) -> Result<()> {
    require!(tick_lower < tick_upper, ErrorCode::TickInvaildOrder);
    Ok(())
}
