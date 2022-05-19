pub mod access_control;
pub mod context;
pub mod error;
pub mod libraries;
pub mod states;
use crate::access_control::*;
use crate::error::ErrorCode;
use crate::libraries::liquidity_amounts;
use crate::libraries::tick_math;
use crate::states::oracle;
use crate::states::oracle::ObservationState;
use crate::states::tokenized_position::{
    CollectTokenizedEvent, DecreaseLiquidityEvent, IncreaseLiquidityEvent,
};
use crate::{
    libraries::{fixed_point_32, swap_math},
    states::{oracle::OBSERVATION_SEED, tick_bitmap},
};
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::system_instruction::create_account;
use anchor_lang::{solana_program::instruction::Instruction, InstructionData};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::token;
use anchor_spl::token::TokenAccount;
use context::*;
use libraries::full_math::MulDiv;
use libraries::liquidity_math;
use libraries::sqrt_price_math;
use metaplex_token_metadata::{instruction::create_metadata_accounts, state::Creator};
use spl_token::instruction::AuthorityType;
use states::factory::*;
use states::fee::*;
use states::pool::*;
use states::position::*;
use states::tick;
use states::tick::*;
use states::tick_bitmap::*;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::mem::size_of;
use std::ops::Neg;
use std::ops::{Deref, DerefMut};

declare_id!("7sSUSz5fEcX6CNrbu3Z3JRdTGqPQdxJYTwKYP8NF95Pp");

#[program]
pub mod cyclos_core {

    use super::*;

    // ---------------------------------------------------------------------
    // Factory instructions
    // The Factory facilitates creation of pools and control over the protocol fees

    /// Initialize the factory state and set the protocol owner
    ///
    /// # Arguments
    ///
    /// * `ctx`- Initializes the factory state account
    /// * `factory_state_bump` - Bump to validate factory state address
    ///
    pub fn init_factory(ctx: Context<Initialize>) -> Result<()> {
        let mut factory_state = ctx.accounts.factory_state.load_init()?;
        factory_state.bump = *ctx.bumps.get("factory_state").unwrap();
        factory_state.owner = ctx.accounts.owner.key();
        factory_state.fee_protocol = 3; // 1/3 = 33.33%

        emit!(OwnerChanged {
            old_owner: Pubkey::default(),
            new_owner: ctx.accounts.owner.key(),
        });

        Ok(())
    }

    /// Updates the owner of the factory
    /// Must be called by the current owner
    ///
    /// # Arguments
    ///
    /// * `ctx`- Checks whether protocol owner has signed
    ///
    pub fn set_owner(ctx: Context<SetOwner>) -> Result<()> {
        let mut factory_state = ctx.accounts.factory_state.load_mut()?;
        factory_state.owner = ctx.accounts.new_owner.key();

        emit!(OwnerChanged {
            old_owner: ctx.accounts.owner.key(),
            new_owner: ctx.accounts.new_owner.key(),
        });

        Ok(())
    }

    /// Enables a fee amount with the given tick_spacing
    /// Fee amounts may never be removed once enabled
    ///
    /// # Arguments
    ///
    /// * `ctx`- Checks whether protocol owner has signed and initializes the fee account
    /// * `fee_state_bump` - Bump to validate fee state address
    /// * `fee` - The fee amount to enable, denominated in hundredths of a bip (i.e. 1e-6)
    /// * `tick_spacing` - The spacing between ticks to be enforced for all pools created
    /// with the given fee amount
    ///
    pub fn enable_fee_amount(
        ctx: Context<EnableFeeAmount>,
        fee: u32,
        tick_spacing: u16,
    ) -> Result<()> {
        assert!(fee < 1_000_000); // 100%

        // TODO examine max value of tick_spacing
        // tick spacing is capped at 16384 to prevent the situation where tick_spacing is so large that
        // tick_bitmap#next_initialized_tick_within_one_word overflows int24 container from a valid tick
        // 16384 ticks represents a >5x price change with ticks of 1 bips
        let mut fee_state = ctx.accounts.fee_state.load_init()?;
        assert!(tick_spacing > 0 && tick_spacing < 16384);
        fee_state.bump = *ctx.bumps.get("fee_state").unwrap();
        fee_state.fee = fee;
        fee_state.tick_spacing = tick_spacing;

        emit!(FeeAmountEnabled { fee, tick_spacing });
        Ok(())
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
    pub fn create_and_init_pool(
        ctx: Context<CreateAndInitPool>,
        sqrt_price_x32: u64,
    ) -> Result<()> {
        let mut pool_state = ctx.accounts.pool_state.load_init()?;
        let fee_state = ctx.accounts.fee_state.load()?;
        let tick = tick_math::get_tick_at_sqrt_ratio(sqrt_price_x32)?;

        pool_state.bump = *ctx.bumps.get("pool_state").unwrap();
        pool_state.token_0 = ctx.accounts.token_0.key();
        pool_state.token_1 = ctx.accounts.token_1.key();
        pool_state.fee = fee_state.fee;
        pool_state.tick_spacing = fee_state.tick_spacing;
        pool_state.sqrt_price_x32 = sqrt_price_x32;
        pool_state.tick = tick;
        pool_state.unlocked = true;
        pool_state.observation_cardinality = 1;
        pool_state.observation_cardinality_next = 1;

        let mut initial_observation_state = ctx.accounts.initial_observation_state.load_init()?;
        initial_observation_state.bump = *ctx.bumps.get("initial_observation_state").unwrap();
        initial_observation_state.block_timestamp = oracle::_block_timestamp();
        initial_observation_state.initialized = true;

        // default value 0 for remaining variables

        emit!(PoolCreatedAndInitialized {
            token_0: ctx.accounts.token_0.key(),
            token_1: ctx.accounts.token_1.key(),
            fee: fee_state.fee,
            tick_spacing: fee_state.tick_spacing,
            pool_state: ctx.accounts.pool_state.key(),
            sqrt_price_x32,
            tick,
        });
        Ok(())
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
        ctx: Context<'a, 'b, 'c, 'info, IncreaseObservationCardinalityNext<'info>>,
        observation_account_bumps: Vec<u8>,
    ) -> Result<()> {
        let mut pool_state = ctx.accounts.pool_state.load_mut()?;
        require!(pool_state.unlocked, ErrorCode::LOK);
        pool_state.unlocked = false;

        let mut i: usize = 0;
        while i < observation_account_bumps.len() {
            let observation_account_seeds = [
                &OBSERVATION_SEED.as_bytes(),
                pool_state.token_0.as_ref(),
                pool_state.token_1.as_ref(),
                &pool_state.fee.to_be_bytes(),
                &(pool_state.observation_cardinality_next + i as u16).to_be_bytes(),
                &[observation_account_bumps[i]],
            ];

            require!(
                ctx.remaining_accounts[i].key()
                    == Pubkey::create_program_address(
                        &observation_account_seeds[..],
                        &ctx.program_id
                    )
                    .unwrap(),
                ErrorCode::OS
            );

            let space = 8 + size_of::<ObservationState>();
            let rent = Rent::get()?;
            let lamports = rent.minimum_balance(space);
            let ix = create_account(
                ctx.accounts.payer.key,
                &ctx.remaining_accounts[i].key,
                lamports,
                space as u64,
                ctx.program_id,
            );

            solana_program::program::invoke_signed(
                &ix,
                &[
                    ctx.accounts.payer.to_account_info(),
                    ctx.remaining_accounts[i].to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
                &[&observation_account_seeds[..]],
            )?;

            let observation_state_loader = AccountLoader::<ObservationState>::try_from_unchecked(
                &cyclos_core::id(),
                &ctx.remaining_accounts[i].to_account_info(),
            )?;
            let mut observation_state = observation_state_loader.load_init()?;
            // this data will not be used because the initialized boolean is still false
            observation_state.bump = observation_account_bumps[i];
            observation_state.index = pool_state.observation_cardinality_next + i as u16;
            observation_state.block_timestamp = 1;

            drop(observation_state);
            observation_state_loader.exit(ctx.program_id)?;

            i += 1;
        }
        let observation_cardinality_next_old = pool_state.observation_cardinality_next;
        pool_state.observation_cardinality_next = pool_state
            .observation_cardinality_next
            .checked_add(i as u16)
            .unwrap();

        emit!(oracle::IncreaseObservationCardinalityNext {
            observation_cardinality_next_old,
            observation_cardinality_next_new: pool_state.observation_cardinality_next,
        });

        pool_state.unlocked = true;
        Ok(())
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
    pub fn set_fee_protocol(ctx: Context<SetFeeProtocol>, fee_protocol: u8) -> Result<()> {
        assert!(fee_protocol >= 2 && fee_protocol <= 10);
        let mut factory_state = ctx.accounts.factory_state.load_mut()?;
        let fee_protocol_old = factory_state.fee_protocol;
        factory_state.fee_protocol = fee_protocol;

        emit!(SetFeeProtocolEvent {
            fee_protocol_old,
            fee_protocol
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
    pub fn collect_protocol(
        ctx: Context<CollectProtocol>,
        amount_0_requested: u64,
        amount_1_requested: u64,
    ) -> Result<()> {
        let mut pool_state = ctx.accounts.pool_state.load_mut()?;
        require!(pool_state.unlocked, ErrorCode::LOK);
        pool_state.unlocked = false;

        let amount_0 = amount_0_requested.min(pool_state.protocol_fees_token_0);
        let amount_1 = amount_1_requested.min(pool_state.protocol_fees_token_1);

        let pool_state_seeds = [
            &POOL_SEED.as_bytes(),
            &pool_state.token_0.to_bytes() as &[u8],
            &pool_state.token_1.to_bytes() as &[u8],
            &pool_state.fee.to_be_bytes(),
            &[pool_state.bump],
        ];

        pool_state.protocol_fees_token_0 -= amount_0;
        pool_state.protocol_fees_token_1 -= amount_1;
        drop(pool_state);

        if amount_0 > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: ctx.accounts.vault_0.to_account_info().clone(),
                        to: ctx.accounts.recipient_wallet_0.to_account_info().clone(),
                        authority: ctx.accounts.pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_0,
            )?;
        }
        if amount_1 > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: ctx.accounts.vault_1.to_account_info().clone(),
                        to: ctx.accounts.recipient_wallet_1.to_account_info().clone(),
                        authority: ctx.accounts.pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_1,
            )?;
        }

        emit!(CollectProtocolEvent {
            pool_state: ctx.accounts.pool_state.key(),
            sender: ctx.accounts.owner.key(),
            recipient_wallet_0: ctx.accounts.recipient_wallet_0.key(),
            recipient_wallet_1: ctx.accounts.recipient_wallet_1.key(),
            amount_0,
            amount_1,
        });

        pool_state = ctx.accounts.pool_state.load_mut()?;
        pool_state.unlocked = true;
        Ok(())
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
    pub fn init_tick_account(ctx: Context<InitTickAccount>, tick: i32) -> Result<()> {
        let pool_state = ctx.accounts.pool_state.load()?;
        check_tick(tick, pool_state.tick_spacing)?;
        let mut tick_state = ctx.accounts.tick_state.load_init()?;
        tick_state.bump = *ctx.bumps.get("tick_state").unwrap();
        tick_state.tick = tick;
        Ok(())
    }

    /// Reclaims lamports from a cleared tick account
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds tick and recipient accounts with validation and closure code
    ///
    pub fn close_tick_account(_ctx: Context<CloseTickAccount>) -> Result<()> {
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
    pub fn init_bitmap_account(ctx: Context<InitBitmapAccount>, word_pos: i16) -> Result<()> {
        let pool_state = ctx.accounts.pool_state.load()?;
        let max_word_pos = ((tick_math::MAX_TICK / pool_state.tick_spacing as i32) >> 8) as i16;
        let min_word_pos = ((tick_math::MIN_TICK / pool_state.tick_spacing as i32) >> 8) as i16;
        require!(word_pos >= min_word_pos, ErrorCode::TLM);
        require!(word_pos <= max_word_pos, ErrorCode::TUM);

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
    pub fn init_position_account(ctx: Context<InitPositionAccount>) -> Result<()> {
        let mut position_account = ctx.accounts.position_state.load_init()?;
        position_account.bump = *ctx.bumps.get("position_state").unwrap();
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Position instructions

    /// Callback to pay tokens for creating or adding liquidity to a position
    ///
    /// Callback function lies in core program instead of non_fungible_position_manager since
    /// reentrancy is disallowed in Solana. Integrators can use a second program to handle callbacks.
    ///
    /// # Arguments
    ///
    /// * `amount_0_owed` - The amount of token_0 due to the pool for the minted liquidity
    /// * `amount_1_owed` - The amount of token_1 due to the pool for the minted liquidity
    ///
    pub fn mint_callback(
        ctx: Context<MintCallback>,
        amount_0_owed: u64,
        amount_1_owed: u64,
    ) -> Result<()> {
        if amount_0_owed > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    token::Transfer {
                        from: ctx.accounts.token_account_0.to_account_info(),
                        to: ctx.accounts.vault_0.to_account_info(),
                        authority: ctx.accounts.minter.to_account_info(),
                    },
                ),
                amount_0_owed,
            )?;
        }
        if amount_1_owed > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    token::Transfer {
                        from: ctx.accounts.token_account_1.to_account_info(),
                        to: ctx.accounts.vault_1.to_account_info(),
                        authority: ctx.accounts.minter.to_account_info(),
                    },
                ),
                amount_1_owed,
            )?;
        }
        Ok(())
    }

    /// Callback to pay the pool tokens owed for the swap.
    /// The caller of this method must be checked to be the core program.
    /// amount_0_delta and amount_1_delta can both be 0 if no tokens were swapped.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Token accounts for payment
    /// * `amount_0_delta` - The amount of token_0 that was sent (negative) or must be received (positive) by the pool by
    /// the end of the swap. If positive, the callback must send that amount of token_0 to the pool.
    /// * `amount_1_delta` - The amount of token_1 that was sent (negative) or must be received (positive) by the pool by
    /// the end of the swap. If positive, the callback must send that amount of token_1 to the pool.
    ///
    pub fn swap_callback(
        ctx: Context<SwapCallback>,
        amount_0_delta: i64,
        amount_1_delta: i64,
    ) -> Result<()> {
        let (exact_input, amount_to_pay) = if amount_0_delta > 0 {
            (
                ctx.accounts.input_vault.mint < ctx.accounts.output_vault.mint,
                amount_0_delta as u64,
            )
        } else {
            (
                ctx.accounts.output_vault.mint < ctx.accounts.input_vault.mint,
                amount_1_delta as u64,
            )
        };
        if exact_input {
            msg!("amount to pay {}, delta 0 {}, delta 1 {}", amount_to_pay, amount_0_delta, amount_1_delta);
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    token::Transfer {
                        from: ctx.accounts.input_token_account.to_account_info(),
                        to: ctx.accounts.input_vault.to_account_info(),
                        authority: ctx.accounts.signer.to_account_info(),
                    },
                ),
                amount_to_pay,
            )?;
        } else {
            msg!("exact output not implemented");
        };

        Ok(())
    }

    /// Adds liquidity for the given pool/recipient/tickLower/tickUpper position
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds the recipient's address and program accounts for
    /// pool, position and ticks.
    /// * `amount` - The amount of liquidity to mint
    ///
    pub fn mint<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, MintContext<'info>>,
        amount: u64,
    ) -> Result<()> {
        let mut pool = ctx.accounts.pool_state.load_mut()?;

        assert!(
            ctx.accounts.vault_0.key()
                == get_associated_token_address(&ctx.accounts.pool_state.key(), &pool.token_0)
        );
        assert!(
            ctx.accounts.vault_1.key()
                == get_associated_token_address(&ctx.accounts.pool_state.key(), &pool.token_1)
        );
        let tick_lower = *ctx.accounts.tick_lower_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_lower_state.key(),
            tick_lower.bump,
            tick_lower.tick,
        )?;

        let tick_upper = *ctx.accounts.tick_upper_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_upper_state.key(),
            tick_upper.bump,
            tick_upper.tick,
        )?;

        let bitmap_lower_state = AccountLoader::<TickBitmapState>::try_from(
            &ctx.accounts.bitmap_lower_state.to_account_info(),
        )?;
        pool.validate_bitmap_address(
            &ctx.accounts.bitmap_lower_state.key(),
            bitmap_lower_state.load()?.bump,
            tick_bitmap::position(tick_lower.tick / pool.tick_spacing as i32).word_pos,
        )?;
        let bitmap_upper_state = AccountLoader::<TickBitmapState>::try_from(
            &ctx.accounts.bitmap_upper_state.to_account_info(),
        )?;
        pool.validate_bitmap_address(
            &ctx.accounts.bitmap_upper_state.key(),
            bitmap_upper_state.load()?.bump,
            tick_bitmap::position(tick_upper.tick / pool.tick_spacing as i32).word_pos,
        )?;

        let position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.position_state.to_account_info(),
        )?;
        pool.validate_position_address(
            &ctx.accounts.position_state.key(),
            position_state.load()?.bump,
            &ctx.accounts.recipient.key(),
            tick_lower.tick,
            tick_upper.tick,
        )?;

        let last_observation_state = AccountLoader::<ObservationState>::try_from(
            &ctx.accounts.last_observation_state.to_account_info(),
        )?;
        pool.validate_observation_address(
            &last_observation_state.key(),
            last_observation_state.load()?.bump,
            false,
        )?;

        require!(pool.unlocked, ErrorCode::LOK);
        pool.unlocked = false;

        assert!(amount > 0);

        let (amount_0_int, amount_1_int) = _modify_position(
            i64::try_from(amount).unwrap(),
            pool.deref_mut(),
            &position_state,
            &ctx.accounts.tick_lower_state,
            &ctx.accounts.tick_upper_state,
            &bitmap_lower_state,
            &bitmap_upper_state,
            &last_observation_state,
            ctx.remaining_accounts,
        )?;

        let amount_0 = amount_0_int as u64;
        let amount_1 = amount_1_int as u64;

        let balance_0_before = if amount_0 > 0 {
            ctx.accounts.vault_0.amount
        } else {
            0
        };
        let balance_1_before = if amount_1 > 0 {
            ctx.accounts.vault_1.amount
        } else {
            0
        };

        drop(pool);

        let mint_callback_ix = cyclos_core::instruction::MintCallback {
            amount_0_owed: amount_0,
            amount_1_owed: amount_1,
        };
        let ix = Instruction::new_with_bytes(
            ctx.accounts.callback_handler.key(),
            &mint_callback_ix.data(),
            ctx.accounts.to_account_metas(None),
        );
        solana_program::program::invoke(&ix, &ctx.accounts.to_account_infos())?;

        ctx.accounts.vault_0.reload()?;
        ctx.accounts.vault_1.reload()?;

        if amount_0 > 0 {
            require!(
                balance_0_before + amount_0 <= ctx.accounts.vault_0.amount,
                ErrorCode::M0
            );
        }
        if amount_1 > 0 {
            require!(
                balance_1_before + amount_1 <= ctx.accounts.vault_1.amount,
                ErrorCode::M1
            );
        }

        emit!(MintEvent {
            pool_state: ctx.accounts.pool_state.key(),
            sender: ctx.accounts.minter.key(),
            owner: ctx.accounts.recipient.key(),
            tick_lower: tick_lower.tick,
            tick_upper: tick_upper.tick,
            amount,
            amount_0,
            amount_1
        });

        ctx.accounts.pool_state.load_mut()?.unlocked = true;
        Ok(())
    }

    /// Burn liquidity from the sender and account tokens owed for the liquidity to the position.
    /// Can be used to trigger a recalculation of fees owed to a position by calling with an amount of 0 (poke).
    /// Fees must be collected separately via a call to #collect
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds position and other validated accounts need to burn liquidity
    /// * `amount` - Amount of liquidity to be burned
    ///
    pub fn burn<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, BurnContext<'info>>,
        amount: u64,
    ) -> Result<()> {
        let pool_state =
            AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
        let mut pool = pool_state.load_mut()?;

        let tick_lower_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
        let tick_lower = *tick_lower_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_lower_state.key(),
            tick_lower.bump,
            tick_lower.tick,
        )?;

        let tick_upper_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
        let tick_upper = *tick_upper_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_upper_state.key(),
            tick_upper.bump,
            tick_upper.tick,
        )?;

        let bitmap_lower_state = AccountLoader::<TickBitmapState>::try_from(
            &ctx.accounts.bitmap_lower_state.to_account_info(),
        )?;
        pool.validate_bitmap_address(
            &ctx.accounts.bitmap_lower_state.key(),
            bitmap_lower_state.load()?.bump,
            tick_bitmap::position(tick_lower.tick / pool.tick_spacing as i32).word_pos,
        )?;
        let bitmap_upper_state = AccountLoader::<TickBitmapState>::try_from(
            &ctx.accounts.bitmap_upper_state.to_account_info(),
        )?;
        pool.validate_bitmap_address(
            &ctx.accounts.bitmap_upper_state.key(),
            bitmap_upper_state.load()?.bump,
            tick_bitmap::position(tick_upper.tick / pool.tick_spacing as i32).word_pos,
        )?;

        let position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.position_state.to_account_info(),
        )?;
        pool.validate_position_address(
            &ctx.accounts.position_state.key(),
            position_state.load()?.bump,
            &ctx.accounts.owner.key(),
            tick_lower.tick,
            tick_upper.tick,
        )?;

        let last_observation_state = AccountLoader::<ObservationState>::try_from(
            &ctx.accounts.last_observation_state.to_account_info(),
        )?;
        pool.validate_observation_address(
            &ctx.accounts.last_observation_state.key(),
            last_observation_state.load()?.bump,
            false,
        )?;

        msg!("accounts validated");

        require!(pool.unlocked, ErrorCode::LOK);
        pool.unlocked = false;

        let (amount_0_int, amount_1_int) = _modify_position(
            -i64::try_from(amount).unwrap(),
            pool.deref_mut(),
            &ctx.accounts.position_state,
            &tick_lower_state,
            &tick_upper_state,
            &bitmap_lower_state,
            &bitmap_upper_state,
            &last_observation_state,
            ctx.remaining_accounts,
        )?;

        let amount_0 = (-amount_0_int) as u64;
        let amount_1 = (-amount_1_int) as u64;
        if amount_0 > 0 || amount_1 > 0 {
            let mut position_state = ctx.accounts.position_state.load_mut()?;
            position_state.tokens_owed_0 += amount_0;
            position_state.tokens_owed_1 += amount_1;
        }

        emit!(BurnEvent {
            pool_state: ctx.accounts.pool_state.key(),
            owner: ctx.accounts.owner.key(),
            tick_lower: tick_lower.tick,
            tick_upper: tick_lower.tick,
            amount,
            amount_0,
            amount_1,
        });

        pool.unlocked = true;
        Ok(())
    }

    /// Collects tokens owed to a position.
    ///
    /// Does not recompute fees earned, which must be done either via mint or burn of any amount of liquidity.
    /// Collect must be called by the position owner. To withdraw only token_0 or only token_1, amount_0_requested or
    /// amount_1_requested may be set to zero. To withdraw all tokens owed, caller may pass any value greater than the
    /// actual tokens owed, e.g. u64::MAX. Tokens owed may be from accumulated swap fees or burned liquidity.
    ///
    /// # Arguments
    ///
    /// * `amount_0_requested` - How much token_0 should be withdrawn from the fees owed
    /// * `amount_1_requested` - How much token_1 should be withdrawn from the fees owed
    ///
    pub fn collect(
        ctx: Context<CollectContext>,
        amount_0_requested: u64,
        amount_1_requested: u64,
    ) -> Result<()> {
        let pool_state =
            AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
        let mut pool = pool_state.load_mut()?;

        let tick_lower_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
        let tick_lower = *tick_lower_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_lower_state.key(),
            tick_lower.bump,
            tick_lower.tick,
        )?;

        let tick_upper_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
        let tick_upper = *tick_upper_state.load()?.deref();
        pool.validate_tick_address(
            &ctx.accounts.tick_upper_state.key(),
            tick_upper.bump,
            tick_upper.tick,
        )?;

        let position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.position_state.to_account_info(),
        )?;
        pool.validate_position_address(
            &ctx.accounts.position_state.key(),
            position_state.load()?.bump,
            &ctx.accounts.owner.key(),
            tick_lower.tick,
            tick_upper.tick,
        )?;

        require!(pool.unlocked, ErrorCode::LOK);
        pool.unlocked = false;

        let mut position = position_state.load_mut()?;

        let amount_0 = amount_0_requested.min(position.tokens_owed_0);
        let amount_1 = amount_1_requested.min(position.tokens_owed_1);

        let pool_state_seeds = [
            &POOL_SEED.as_bytes(),
            &pool.token_0.to_bytes() as &[u8],
            &pool.token_1.to_bytes() as &[u8],
            &pool.fee.to_be_bytes(),
            &[pool.bump],
        ];

        drop(pool);
        if amount_0 > 0 {
            position.tokens_owed_0 -= amount_0;
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: ctx.accounts.vault_0.to_account_info().clone(),
                        to: ctx.accounts.recipient_wallet_0.to_account_info().clone(),
                        authority: pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_0,
            )?;
        }
        if amount_1 > 0 {
            position.tokens_owed_1 -= amount_1;
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info().clone(),
                    token::Transfer {
                        from: ctx.accounts.vault_1.to_account_info().clone(),
                        to: ctx.accounts.recipient_wallet_1.to_account_info().clone(),
                        authority: pool_state.to_account_info().clone(),
                    },
                    &[&pool_state_seeds[..]],
                ),
                amount_1,
            )?;
        }

        emit!(CollectEvent {
            pool_state: pool_state.key(),
            owner: ctx.accounts.owner.key(),
            tick_lower: tick_lower.tick,
            tick_upper: tick_upper.tick,
            amount_0,
            amount_1,
        });

        pool_state.load_mut()?.unlocked = true;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 4. Swap instructions

    pub struct SwapCache {
        // the protocol fee for the input token
        pub fee_protocol: u8,
        // liquidity at the beginning of the swap
        pub liquidity_start: u64,
        // the timestamp of the current block
        pub block_timestamp: u32,
        // the current value of the tick accumulator, computed only if we cross an initialized tick
        pub tick_cumulative: i64,
        // the current value of seconds per liquidity accumulator, computed only if we cross an initialized tick
        pub seconds_per_liquidity_cumulative_x32: u64,
        // whether we've computed and cached the above two accumulators
        pub computed_latest_observation: bool,
    }

    // the top level state of the swap, the results of which are recorded in storage at the end
    #[derive(Debug)]
    pub struct SwapState {
        // the amount remaining to be swapped in/out of the input/output asset
        pub amount_specified_remaining: i64,
        // the amount already swapped out/in of the output/input asset
        pub amount_calculated: i64,
        // current sqrt(price)
        pub sqrt_price_x32: u64,
        // the tick associated with the current price
        pub tick: i32,
        // the global fee growth of the input token
        pub fee_growth_global_x32: u64,
        // amount of input token paid as protocol fee
        pub protocol_fee: u64,
        // the current liquidity in range
        pub liquidity: u64,
    }

    #[derive(Default)]
    struct StepComputations {
        // the price at the beginning of the step
        sqrt_price_start_x32: u64,
        // the next tick to swap to from the current tick in the swap direction
        tick_next: i32,
        // whether tick_next is initialized or not
        initialized: bool,
        // sqrt(price) for the next tick (1/0)
        sqrt_price_next_x32: u64,
        // how much is being swapped in in this step
        amount_in: u64,
        // how much is being swapped out
        amount_out: u64,
        // how much fee is being paid in
        fee_amount: u64,
    }

    /// Swap token_0 for token_1, or token_1 for token_0
    ///
    /// Outstanding tokens must be paid in #swap_callback
    ///
    /// # Arguments
    ///
    /// * `ctx` - Accounts required for the swap. Remaining accounts should contain each bitmap leading to
    /// the end tick, and each tick being flipped
    /// account leading to the destination tick
    /// * `deadline` - The time by which the transaction must be included to effect the change
    /// * `amount_specified` - The amount of the swap, which implicitly configures the swap as exact input (positive),
    /// or exact output (negative)
    /// * `sqrt_price_limit` - The Q32.32 sqrt price √P limit. If zero for one, the price cannot
    /// be less than this value after the swap.  If one for zero, the price cannot be greater than
    /// this value after the swap.
    ///
    pub fn swap(
        ctx: Context<SwapContext>,
        amount_specified: i64,
        sqrt_price_limit_x32: u64,
    ) -> Result<()> {
        require!(amount_specified != 0, ErrorCode::AS);

        let factory_state =
            AccountLoader::<FactoryState>::try_from(&ctx.accounts.factory_state.to_account_info())?;

        let pool_loader =
            AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
        let mut pool = pool_loader.load_mut()?;

        let input_token_account =
            Account::<TokenAccount>::try_from(&ctx.accounts.input_token_account)?;
        let output_token_account =
            Account::<TokenAccount>::try_from(&ctx.accounts.output_token_account)?;

        let zero_for_one = ctx.accounts.input_vault.mint == pool.token_0;

        let (token_account_0, token_account_1, mut vault_0, mut vault_1) = if zero_for_one {
            (
                input_token_account,
                output_token_account,
                ctx.accounts.input_vault.clone(),
                ctx.accounts.output_vault.clone(),
            )
        } else {
            (
                output_token_account,
                input_token_account,
                ctx.accounts.output_vault.clone(),
                ctx.accounts.input_vault.clone(),
            )
        };
        assert!(vault_0.key() == get_associated_token_address(&pool_loader.key(), &pool.token_0));
        assert!(vault_1.key() == get_associated_token_address(&pool_loader.key(), &pool.token_1));

        let last_observation_state = AccountLoader::<ObservationState>::try_from(
            &ctx.accounts.last_observation_state.to_account_info(),
        )?;
        pool.validate_observation_address(
            &ctx.accounts.last_observation_state.key(),
            last_observation_state.load()?.bump,
            false,
        )?;

        require!(pool.unlocked, ErrorCode::LOK);
        require!(
            if zero_for_one {
                sqrt_price_limit_x32 < pool.sqrt_price_x32
                    && sqrt_price_limit_x32 > tick_math::MIN_SQRT_RATIO
            } else {
                sqrt_price_limit_x32 > pool.sqrt_price_x32
                    && sqrt_price_limit_x32 < tick_math::MAX_SQRT_RATIO
            },
            ErrorCode::SPL
        );

        pool.unlocked = false;
        let mut cache = SwapCache {
            liquidity_start: pool.liquidity,
            block_timestamp: oracle::_block_timestamp(),
            fee_protocol: factory_state.load()?.fee_protocol,
            seconds_per_liquidity_cumulative_x32: 0,
            tick_cumulative: 0,
            computed_latest_observation: false,
        };

        let exact_input = amount_specified > 0;

        let mut state = SwapState {
            amount_specified_remaining: amount_specified,
            amount_calculated: 0,
            sqrt_price_x32: pool.sqrt_price_x32,
            tick: pool.tick,
            fee_growth_global_x32: if zero_for_one {
                pool.fee_growth_global_0_x32
            } else {
                pool.fee_growth_global_1_x32
            },
            protocol_fee: 0,
            liquidity: cache.liquidity_start,
        };

        let latest_observation = last_observation_state.load_mut()?;
        let mut remaining_accounts = ctx.remaining_accounts.iter();

        // cache for the current bitmap account. Cache is cleared on bitmap transitions
        let mut bitmap_cache: Option<TickBitmapState> = None;

        // continue swapping as long as we haven't used the entire input/output and haven't
        // reached the price limit
        while state.amount_specified_remaining != 0 && state.sqrt_price_x32 != sqrt_price_limit_x32
        {
            let mut step = StepComputations::default();
            step.sqrt_price_start_x32 = state.sqrt_price_x32;

            let mut compressed = state.tick / pool.tick_spacing as i32;

            // state.tick is the starting tick for the transition
            if state.tick < 0 && state.tick % pool.tick_spacing as i32 != 0 {
                compressed -= 1; // round towards negative infinity
            }
            // The current tick is not considered in greater than or equal to (lte = false, i.e one for zero) case
            if !zero_for_one {
                compressed += 1;
            }

            let Position { word_pos, bit_pos } = tick_bitmap::position(compressed);

            // load the next bitmap account if cache is empty (first loop instance), or if we have
            // crossed out of this bitmap
            if bitmap_cache.is_none() || bitmap_cache.unwrap().word_pos != word_pos {
                let bitmap_account = remaining_accounts.next().unwrap();
                msg!("check bitmap {}", word_pos);
                // ensure this is a valid PDA, even if account is not initialized
                assert!(
                    bitmap_account.key()
                        == Pubkey::find_program_address(
                            &[
                                BITMAP_SEED.as_bytes(),
                                pool.token_0.as_ref(),
                                pool.token_1.as_ref(),
                                &pool.fee.to_be_bytes(),
                                &word_pos.to_be_bytes(),
                            ],
                            &cyclos_core::id()
                        )
                        .0
                );

                // read from bitmap if account is initialized, else use default values for next initialized bit
                if let Ok(bitmap_loader) =
                    AccountLoader::<TickBitmapState>::try_from(bitmap_account)
                {
                    let bitmap_state = bitmap_loader.load()?;
                    bitmap_cache = Some(*bitmap_state.deref());
                } else {
                    // clear cache if the bitmap account was uninitialized. This way default uninitialized
                    // values will be returned for the next bit
                    msg!("cache cleared");
                    bitmap_cache = None;
                }
            }

            // what if bitmap_cache is not updated since next account is not initialized?
            // default values for the next initialized bit if the bitmap account is not initialized
            let next_initialized_bit = if let Some(bitmap) = bitmap_cache {
                bitmap.next_initialized_bit(bit_pos, zero_for_one)
            } else {
                NextBit {
                    next: if zero_for_one { 0 } else { 255 },
                    initialized: false,
                }
            };

            step.tick_next = (((word_pos as i32) << 8) + next_initialized_bit.next as i32)
                * pool.tick_spacing as i32; // convert relative to absolute
            step.initialized = next_initialized_bit.initialized;

            // ensure that we do not overshoot the min/max tick, as the tick bitmap is not aware of these bounds
            if step.tick_next < tick_math::MIN_TICK {
                step.tick_next = tick_math::MIN_TICK;
            } else if step.tick_next > tick_math::MAX_TICK {
                step.tick_next = tick_math::MAX_TICK;
            }

            step.sqrt_price_next_x32 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?;

            let target_price = if (zero_for_one && step.sqrt_price_next_x32 < sqrt_price_limit_x32)
                || (!zero_for_one && step.sqrt_price_next_x32 > sqrt_price_limit_x32)
            {
                sqrt_price_limit_x32
            } else {
                step.sqrt_price_next_x32
            };
            let swap_step = swap_math::compute_swap_step(
                state.sqrt_price_x32,
                target_price,
                state.liquidity,
                state.amount_specified_remaining,
                pool.fee,
            );
            state.sqrt_price_x32 = swap_step.sqrt_ratio_next_x32;
            step.amount_in = swap_step.amount_in;
            step.amount_out = swap_step.amount_out;
            step.fee_amount = swap_step.fee_amount;

            if exact_input {
                state.amount_specified_remaining -=
                    i64::try_from(step.amount_in + step.fee_amount).unwrap();
                state.amount_calculated = state
                    .amount_calculated
                    .checked_sub(i64::try_from(step.amount_out).unwrap())
                    .unwrap();
            } else {
                state.amount_specified_remaining += i64::try_from(step.amount_out).unwrap();
                state.amount_calculated = state
                    .amount_calculated
                    .checked_add(i64::try_from(step.amount_in + step.fee_amount).unwrap())
                    .unwrap();
            }

            // if the protocol fee is on, calculate how much is owed, decrement fee_amount, and increment protocol_fee
            if cache.fee_protocol > 0 {
                let delta = step.fee_amount / cache.fee_protocol as u64;
                step.fee_amount -= delta;
                state.protocol_fee += delta;
            }

            // update global fee tracker
            if state.liquidity > 0 {
                state.fee_growth_global_x32 += step
                    .fee_amount
                    .mul_div_floor(fixed_point_32::Q32, state.liquidity)
                    .unwrap();
            }

            // shift tick if we reached the next price
            if state.sqrt_price_x32 == step.sqrt_price_next_x32 {
                // if the tick is initialized, run the tick transition
                if step.initialized {
                    // check for the placeholder value for the oracle observation, which we replace with the
                    // actual value the first time the swap crosses an initialized tick
                    if !cache.computed_latest_observation {
                        let new_observation = latest_observation.observe_latest(
                            cache.block_timestamp,
                            pool.tick,
                            pool.liquidity,
                        );
                        cache.tick_cumulative = new_observation.0;
                        cache.seconds_per_liquidity_cumulative_x32 = new_observation.1;
                        cache.computed_latest_observation = true;
                    }

                    msg!("loading tick {}", step.tick_next);
                    let tick_loader =
                        AccountLoader::<TickState>::try_from(remaining_accounts.next().unwrap())?;
                    let mut tick_state = tick_loader.load_mut()?;
                    pool.validate_tick_address(
                        &tick_loader.key(),
                        tick_state.bump,
                        step.tick_next,
                    )?;
                    let mut liquidity_net = tick_state.deref_mut().cross(
                        if zero_for_one {
                            state.fee_growth_global_x32
                        } else {
                            pool.fee_growth_global_0_x32
                        },
                        if zero_for_one {
                            pool.fee_growth_global_1_x32
                        } else {
                            state.fee_growth_global_x32
                        },
                        cache.seconds_per_liquidity_cumulative_x32,
                        cache.tick_cumulative,
                        cache.block_timestamp,
                    );

                    // if we're moving leftward, we interpret liquidity_net as the opposite sign
                    // safe because liquidity_net cannot be i64::MIN
                    if zero_for_one {
                        liquidity_net = liquidity_net.neg();
                    }

                    state.liquidity = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
                }

                state.tick = if zero_for_one {
                    step.tick_next - 1
                } else {
                    step.tick_next
                };
            } else if state.sqrt_price_x32 != step.sqrt_price_start_x32 {
                // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
                state.tick = tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x32)?;
            }
        }
        let partition_current_timestamp = cache.block_timestamp / 14;
        let partition_last_timestamp = latest_observation.block_timestamp / 14;
        drop(latest_observation);

        // update tick and write an oracle entry if the tick changes
        if state.tick != pool.tick {
            // use the next observation account and update pool observation index if block time falls
            // in another partition
            let next_observation_state;
            let mut next_observation = if partition_current_timestamp > partition_last_timestamp {
                next_observation_state = AccountLoader::<ObservationState>::try_from(
                    &remaining_accounts.next().unwrap(),
                )?;
                let next_observation = next_observation_state.load_mut()?;

                pool.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation.bump,
                    true,
                )?;

                next_observation
            } else {
                last_observation_state.load_mut()?
            };
            pool.tick = state.tick;
            pool.observation_cardinality_next = next_observation.update(
                cache.block_timestamp,
                pool.tick,
                cache.liquidity_start,
                pool.observation_cardinality,
                pool.observation_cardinality_next,
            );
        }
        pool.sqrt_price_x32 = state.sqrt_price_x32;

        // update liquidity if it changed
        if cache.liquidity_start != state.liquidity {
            pool.liquidity = state.liquidity;
        }

        // update fee growth global and, if necessary, protocol fees
        // overflow is acceptable, protocol has to withdraw before it hit u64::MAX fees
        if zero_for_one {
            pool.fee_growth_global_0_x32 = state.fee_growth_global_x32;
            if state.protocol_fee > 0 {
                pool.protocol_fees_token_0 += state.protocol_fee;
            }
        } else {
            pool.fee_growth_global_1_x32 = state.fee_growth_global_x32;
            if state.protocol_fee > 0 {
                pool.protocol_fees_token_1 += state.protocol_fee;
            }
        }

        let (amount_0, amount_1) = if zero_for_one == exact_input {
            (
                amount_specified - state.amount_specified_remaining,
                state.amount_calculated,
            )
        } else {
            (
                state.amount_calculated,
                amount_specified - state.amount_specified_remaining,
            )
        };

        // do the transfers and collect payment
        let pool_state_seeds = [
            &POOL_SEED.as_bytes(),
            &pool.token_0.to_bytes() as &[u8],
            &pool.token_1.to_bytes() as &[u8],
            &pool.fee.to_be_bytes(),
            &[pool.bump],
        ];
        drop(pool);

        msg!("vault balances {} {}", vault_0.amount, vault_1.amount);

        if zero_for_one {
            if amount_1 < 0 {
                msg!("paying {}", amount_1.neg());
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info().clone(),
                        token::Transfer {
                            from: vault_1.to_account_info().clone(),
                            to: token_account_1.to_account_info().clone(),
                            authority: ctx.accounts.pool_state.to_account_info().clone(),
                        },
                        &[&pool_state_seeds[..]],
                    ),
                    amount_1.neg() as u64,
                )?;
            }
            let balance_0_before = vault_0.amount;

            // transfer tokens to pool in callback
            let swap_callback_ix = cyclos_core::instruction::SwapCallback {
                amount_0_delta: amount_0,
                amount_1_delta: amount_1,
            };
            let ix = Instruction::new_with_bytes(
                ctx.accounts.callback_handler.key(),
                &swap_callback_ix.data(),
                ctx.accounts.to_account_metas(None),
            );
            solana_program::program::invoke(&ix, &ctx.accounts.to_account_infos())?;
            vault_0.reload()?;
            require!(
                balance_0_before.checked_add(amount_0 as u64).unwrap() <= vault_0.amount,
                ErrorCode::IIA
            );
        } else {
            if amount_0 < 0 {
                msg!("paying {}", amount_0.neg());
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info().clone(),
                        token::Transfer {
                            from: vault_0.to_account_info().clone(),
                            to: token_account_0.to_account_info().clone(),
                            authority: ctx.accounts.pool_state.to_account_info().clone(),
                        },
                        &[&pool_state_seeds[..]],
                    ),
                    amount_0.neg() as u64,
                )?;
            }
            let balance_1_before = vault_1.amount;
            // transfer tokens to pool in callback
            let swap_callback_ix = cyclos_core::instruction::SwapCallback {
                amount_0_delta: amount_0,
                amount_1_delta: amount_1,
            };
            let ix = Instruction::new_with_bytes(
                ctx.accounts.callback_handler.key(),
                &swap_callback_ix.data(),
                ctx.accounts.to_account_metas(None),
            );
            solana_program::program::invoke(&ix, &ctx.accounts.to_account_infos())?;
            vault_1.reload()?;
            require!(
                balance_1_before.checked_add(amount_1 as u64).unwrap() <= vault_1.amount,
                ErrorCode::IIA
            );
        }

        emit!(SwapEvent {
            pool_state: pool_loader.key(),
            sender: ctx.accounts.signer.key(),
            token_account_0: token_account_0.key(),
            token_account_1: token_account_1.key(),
            amount_0,
            amount_1,
            sqrt_price_x32: state.sqrt_price_x32,
            liquidity: state.liquidity,
            tick: state.tick
        });
        pool_loader.load_mut()?.unlocked = true;

        Ok(())
    }

    // /// Component function for flash swaps
    // ///
    // /// Donate given liquidity to in-range positions then make callback
    // /// Only callable by a smart contract which implements uniswapV3FlashCallback(),
    // /// where profitability check can be performed
    // ///
    // /// Flash swaps is an advanced feature for developers, not directly available for UI based traders.
    // /// Periphery does not provide an implementation, but a sample is provided
    // /// Ref- https://github.com/Uniswap/v3-periphery/blob/main/contracts/examples/PairFlash.sol
    // ///
    // ///
    // /// Flow
    // /// 1. FlashDapp.initFlash()
    // /// 2. Core.flash()
    // /// 3. FlashDapp.uniswapV3FlashCallback()
    // ///
    // /// @param amount_0 Amount of token 0 to donate
    // /// @param amount_1 Amount of token 1 to donate
    // pub fn flash(ctx: Context<SetFeeProtocol>, amount_0: u64, amount_1: u64) -> Result<()> {
    //     todo!()
    // }

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
    #[access_control(check_deadline(deadline))]
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
            callback_handler: UncheckedAccount::try_from(
                ctx.accounts.core_program.to_account_info(),
            ),
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

    /// Attach metaplex metadata to a tokenized position. Permissionless to call.
    /// Optional and cosmetic in nature.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Holds validated metadata account and tokenized position addresses
    ///
    pub fn add_metaplex_metadata(ctx: Context<AddMetaplexMetadata>) -> Result<()> {
        let seeds = [&[ctx.accounts.factory_state.load()?.bump] as &[u8]];
        let create_metadata_ix = create_metadata_accounts(
            ctx.accounts.metadata_program.key(),
            ctx.accounts.metadata_account.key(),
            ctx.accounts.nft_mint.key(),
            ctx.accounts.factory_state.key(),
            ctx.accounts.payer.key(),
            ctx.accounts.factory_state.key(),
            String::from("Raydium AMM Positions NFT-V1"),
            String::from("RAY-SOL"), // TODO
            format!(
                "https://asia-south1-raydium-finance.cloudfunctions.net/nft?mint={}", // TODO
                ctx.accounts.nft_mint.key()
            ),
            Some(vec![Creator {
                address: ctx.accounts.factory_state.key(),
                verified: true,
                share: 100,
            }]),
            0,
            true,
            false,
        );
        solana_program::program::invoke_signed(
            &create_metadata_ix,
            &[
                ctx.accounts.metadata_account.to_account_info().clone(),
                ctx.accounts.nft_mint.to_account_info().clone(),
                ctx.accounts.payer.to_account_info().clone(),
                ctx.accounts.factory_state.to_account_info().clone(), // mint and update authority
                ctx.accounts.system_program.to_account_info().clone(),
                ctx.accounts.rent.to_account_info().clone(),
            ],
            &[&seeds[..]],
        )?;

        // Disable minting
        token::set_authority(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(),
                token::SetAuthority {
                    current_authority: ctx.accounts.factory_state.to_account_info().clone(),
                    account_or_mint: ctx.accounts.nft_mint.to_account_info().clone(),
                },
                &[&seeds[..]],
            ),
            AuthorityType::MintTokens,
            None,
        )?;

        Ok(())
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
    #[access_control(check_deadline(deadline))]
    pub fn increase_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, IncreaseLiquidity<'info>>,
        amount_0_desired: u64,
        amount_1_desired: u64,
        amount_0_min: u64,
        amount_1_min: u64,
        deadline: i64,
    ) -> Result<()> {
        let pool_state =
            AccountLoader::<PoolState>::try_from(&ctx.accounts.pool_state.to_account_info())?;
        let tick_lower_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_lower_state.to_account_info())?;
        let tick_lower = tick_lower_state.load()?.tick;

        let tick_upper_state =
            AccountLoader::<TickState>::try_from(&ctx.accounts.tick_upper_state.to_account_info())?;
        let tick_upper = tick_upper_state.load()?.tick;

        let mut accs = MintContext {
            minter: ctx.accounts.payer.clone(),
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
            callback_handler: UncheckedAccount::try_from(
                ctx.accounts.core_program.to_account_info(),
            ),
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

        let core_position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.core_position_state.to_account_info(),
        )?;
        let fee_growth_inside_0_last_x32 = core_position_state.load()?.fee_growth_inside_0_last_x32;
        let fee_growth_inside_1_last_x32 = core_position_state.load()?.fee_growth_inside_1_last_x32;

        // Update tokenized position metadata
        let mut position = ctx.accounts.tokenized_position_state.load_mut()?;
        position.tokens_owed_0 += (fee_growth_inside_0_last_x32
            - position.fee_growth_inside_0_last_x32)
            .mul_div_floor(position.liquidity, fixed_point_32::Q32)
            .unwrap();

        position.tokens_owed_1 += (fee_growth_inside_1_last_x32
            - position.fee_growth_inside_1_last_x32)
            .mul_div_floor(position.liquidity, fixed_point_32::Q32)
            .unwrap();

        position.fee_growth_inside_0_last_x32 = fee_growth_inside_0_last_x32;
        position.fee_growth_inside_1_last_x32 = fee_growth_inside_1_last_x32;
        position.liquidity += liquidity;

        emit!(IncreaseLiquidityEvent {
            token_id: position.mint,
            liquidity,
            amount_0,
            amount_1
        });

        Ok(())
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
    #[access_control(check_deadline(deadline))]
    #[access_control(is_authorized_for_token(&ctx.accounts.owner_or_delegate, &ctx.accounts.nft_account))]
    pub fn decrease_liquidity<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, DecreaseLiquidity<'info>>,
        liquidity: u64,
        amount_0_min: u64,
        amount_1_min: u64,
        deadline: i64,
    ) -> Result<()> {
        assert!(liquidity > 0);

        let position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.core_position_state.to_account_info(),
        )?;
        let tokens_owed_0_before = position_state.load()?.tokens_owed_0;
        let tokens_owed_1_before = position_state.load()?.tokens_owed_1;

        let mut core_position_owner = ctx.accounts.factory_state.to_account_info();
        core_position_owner.is_signer = true;
        let mut accounts = BurnContext {
            owner: Signer::try_from(&core_position_owner)?,
            pool_state: ctx.accounts.pool_state.clone(),
            tick_lower_state: ctx.accounts.tick_lower_state.clone(),
            tick_upper_state: ctx.accounts.tick_upper_state.clone(),
            bitmap_lower_state: ctx.accounts.bitmap_lower_state.clone(),
            bitmap_upper_state: ctx.accounts.bitmap_upper_state.clone(),
            position_state,
            last_observation_state: ctx.accounts.last_observation_state.clone(),
        };
        burn(
            Context::new(
                &ID,
                &mut accounts,
                ctx.remaining_accounts,
                BTreeMap::default(),
            ),
            liquidity,
        )?;
        let updated_core_position = accounts.position_state.load()?;
        let amount_0 = updated_core_position.tokens_owed_0 - tokens_owed_0_before;
        let amount_1 = updated_core_position.tokens_owed_1 - tokens_owed_1_before;
        require!(
            amount_0 >= amount_0_min && amount_1 >= amount_1_min,
            ErrorCode::PriceSlippageCheck
        );

        // Update the tokenized position to the current transaction
        let fee_growth_inside_0_last_x32 = updated_core_position.fee_growth_inside_0_last_x32;
        let fee_growth_inside_1_last_x32 = updated_core_position.fee_growth_inside_1_last_x32;

        let mut tokenized_position = ctx.accounts.tokenized_position_state.load_mut()?;
        tokenized_position.tokens_owed_0 += amount_0
            + (fee_growth_inside_0_last_x32 - tokenized_position.fee_growth_inside_0_last_x32)
                .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                .unwrap();

        tokenized_position.tokens_owed_1 += amount_1
            + (fee_growth_inside_1_last_x32 - tokenized_position.fee_growth_inside_1_last_x32)
                .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                .unwrap();

        tokenized_position.fee_growth_inside_0_last_x32 = fee_growth_inside_0_last_x32;
        tokenized_position.fee_growth_inside_1_last_x32 = fee_growth_inside_1_last_x32;
        tokenized_position.liquidity -= liquidity;

        emit!(DecreaseLiquidityEvent {
            token_id: tokenized_position.mint,
            liquidity,
            amount_0,
            amount_1
        });

        Ok(())
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
    pub fn collect_from_tokenized<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, CollectFromTokenized<'info>>,
        amount_0_max: u64,
        amount_1_max: u64,
    ) -> Result<()> {
        assert!(amount_0_max > 0 || amount_1_max > 0);

        let mut tokenized_position = ctx.accounts.tokenized_position_state.load_mut()?;
        let mut tokens_owed_0 = tokenized_position.tokens_owed_0;
        let mut tokens_owed_1 = tokenized_position.tokens_owed_1;

        let position_state = AccountLoader::<PositionState>::try_from(
            &ctx.accounts.core_position_state.to_account_info(),
        )?;

        // trigger an update of the position fees owed and fee growth snapshots if it has any liquidity
        if tokenized_position.liquidity > 0 {
            let mut core_position_owner = ctx.accounts.factory_state.to_account_info();
            core_position_owner.is_signer = true;
            let mut burn_accounts = BurnContext {
                owner: Signer::try_from(&core_position_owner)?,
                pool_state: ctx.accounts.pool_state.clone(),
                tick_lower_state: ctx.accounts.tick_lower_state.clone(),
                tick_upper_state: ctx.accounts.tick_upper_state.clone(),
                bitmap_lower_state: ctx.accounts.bitmap_lower_state.clone(),
                bitmap_upper_state: ctx.accounts.bitmap_upper_state.clone(),
                position_state,
                last_observation_state: ctx.accounts.last_observation_state.clone(),
            };
            burn(
                Context::new(
                    &ID,
                    &mut burn_accounts,
                    ctx.remaining_accounts,
                    BTreeMap::default(),
                ),
                0,
            )?;

            let core_position = *burn_accounts.position_state.load()?.deref();

            tokens_owed_0 += (core_position.fee_growth_inside_0_last_x32
                - tokenized_position.fee_growth_inside_0_last_x32)
                .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                .unwrap();
            tokens_owed_1 += (core_position.fee_growth_inside_1_last_x32
                - tokenized_position.fee_growth_inside_1_last_x32)
                .mul_div_floor(tokenized_position.liquidity, fixed_point_32::Q32)
                .unwrap();

            tokenized_position.fee_growth_inside_0_last_x32 =
                core_position.fee_growth_inside_0_last_x32;
            tokenized_position.fee_growth_inside_1_last_x32 =
                core_position.fee_growth_inside_1_last_x32;
        }

        // adjust amounts to the max for the position
        let amount_0 = amount_0_max.min(tokens_owed_0);
        let amount_1 = amount_1_max.min(tokens_owed_1);

        let mut core_position_owner = ctx.accounts.factory_state.to_account_info().clone();
        core_position_owner.is_signer = true;

        msg!("withdrawing amounts {} {}", amount_0, amount_1);
        msg!("vault balances {} {}", ctx.accounts.vault_0.amount, ctx.accounts.vault_1.amount);

        let mut accounts = CollectContext {
            owner: Signer::try_from(&core_position_owner)?,
            pool_state: ctx.accounts.pool_state.clone(),
            tick_lower_state: ctx.accounts.tick_lower_state.clone(),
            tick_upper_state: ctx.accounts.tick_upper_state.clone(),
            position_state: ctx.accounts.core_position_state.clone(),
            vault_0: ctx.accounts.vault_0.clone(),
            vault_1: ctx.accounts.vault_1.clone(),
            recipient_wallet_0: ctx.accounts.recipient_wallet_0.clone(),
            recipient_wallet_1: ctx.accounts.recipient_wallet_1.clone(),
            token_program: ctx.accounts.token_program.clone(),
        };
        collect(
            Context::new(&ID, &mut accounts, &[], BTreeMap::default()),
            amount_0,
            amount_1,
        )?;

        // sometimes there will be a few less wei than expected due to rounding down in core, but
        // we just subtract the full amount expected
        // instead of the actual amount so we can burn the token
        tokenized_position.tokens_owed_0 = tokens_owed_0 - amount_0;
        tokenized_position.tokens_owed_1 = tokens_owed_1 - amount_1;

        emit!(CollectTokenizedEvent {
            token_id: tokenized_position.mint,
            recipient_wallet_0: ctx.accounts.recipient_wallet_0.key(),
            recipient_wallet_1: ctx.accounts.recipient_wallet_1.key(),
            amount_0,
            amount_1
        });

        Ok(())
    }

    /// Swaps `amount_in` of one token for as much as possible of another token,
    /// across a single pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - Accounts required for the swap
    /// * `deadline` - The time by which the transaction must be included to effect the change
    /// * `amount_in` - Token amount to be swapped in
    /// * `amount_out_minimum` - The minimum amount to swap out, which serves as a slippage check
    /// * `sqrt_price_limit` - The Q32.32 sqrt price √P limit. If zero for one, the price cannot
    /// be less than this value after the swap.  If one for zero, the price cannot be greater than
    /// this value after the swap.
    ///
    #[access_control(check_deadline(deadline))]
    pub fn exact_input_single<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ExactInputSingle<'info>>,
        deadline: i64,
        amount_in: u64,
        amount_out_minimum: u64,
        sqrt_price_limit_x32: u64,
    ) -> Result<()> {
        let amount_out = exact_input_internal(
            &mut SwapContext {
                signer: ctx.accounts.signer.clone(),
                factory_state: ctx.accounts.factory_state.clone(),
                input_token_account: ctx.accounts.input_token_account.clone(),
                output_token_account: ctx.accounts.output_token_account.clone(),
                input_vault: ctx.accounts.input_vault.clone(),
                output_vault: ctx.accounts.output_vault.clone(),
                token_program: ctx.accounts.token_program.clone(),
                pool_state: ctx.accounts.pool_state.clone(),
                last_observation_state: ctx.accounts.last_observation_state.clone(),
                // next_observation_state: ctx.accounts.next_observation_state.clone(),
                callback_handler: UncheckedAccount::try_from(
                    ctx.accounts.core_program.to_account_info(),
                ),
            },
            ctx.remaining_accounts,
            amount_in,
            sqrt_price_limit_x32,
        )?;
        require!(
            amount_out >= amount_out_minimum,
            ErrorCode::TooLittleReceived
        );
        Ok(())
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
    #[access_control(check_deadline(deadline))]
    pub fn exact_input<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ExactInput<'info>>,
        deadline: i64,
        amount_in: u64,
        amount_out_minimum: u64,
        additional_accounts_per_pool: Vec<u8>,
    ) -> Result<()> {
        let mut remaining_accounts = ctx.remaining_accounts.iter();

        let mut amount_in_internal = amount_in;
        let mut input_token_account = ctx.accounts.input_token_account.clone();
        for i in 0..additional_accounts_per_pool.len() {
            let pool_state = UncheckedAccount::try_from(remaining_accounts.next().unwrap().clone());
            let output_token_account =
                UncheckedAccount::try_from(remaining_accounts.next().unwrap().clone());
            let input_vault = Box::new(Account::<TokenAccount>::try_from(
                remaining_accounts.next().unwrap(),
            )?);
            let output_vault = Box::new(Account::<TokenAccount>::try_from(
                remaining_accounts.next().unwrap(),
            )?);

            amount_in_internal = exact_input_internal(
                &mut SwapContext {
                    signer: ctx.accounts.signer.clone(),
                    factory_state: ctx.accounts.factory_state.clone(),
                    input_token_account: input_token_account.clone(),
                    pool_state,
                    output_token_account: output_token_account.clone(),
                    input_vault,
                    output_vault,
                    last_observation_state: UncheckedAccount::try_from(
                        remaining_accounts.next().unwrap().clone(),
                    ),
                    token_program: ctx.accounts.token_program.clone(),
                    callback_handler: UncheckedAccount::try_from(
                        ctx.accounts.core_program.to_account_info(),
                    ),
                },
                remaining_accounts.as_slice(),
                amount_in_internal,
                0,
            )?;

            if i < additional_accounts_per_pool.len() - 1 {
                // reach accounts needed for the next swap
                for _j in 0..additional_accounts_per_pool[i] {
                    remaining_accounts.next();
                }
                // output token account is the new input
                input_token_account = output_token_account;
            }
        }
        require!(
            amount_in_internal >= amount_out_minimum,
            ErrorCode::TooLittleReceived
        );

        Ok(())
    }

    //  /// Swaps as little as possible of one token for `amount_out` of another token,
    // /// across a single pool
    // ///
    // /// # Arguments
    // ///
    // /// * `ctx` - Token and pool accounts for swap
    // /// * `zero_for_one` - Direction of swap. Swap token_0 for token_1 if true
    // /// * `deadline` - Swap should if fail if past deadline
    // /// * `amount_out` - Token amount to be swapped out
    // /// * `amount_in_maximum` - For slippage. Panic if required input exceeds max limit.
    // /// * `sqrt_price_limit` - Limit price √P for slippage
    // ///
    // pub fn exact_output_single(
    //     ctx: Context<ExactInputSingle>,
    //     zero_for_one: bool,
    //     deadline: u64,
    //     amount_out: u64,
    //     amount_in_maximum: u64,
    //     sqrt_price_limit_x32: u64,
    // ) -> Result<()> {
    //     todo!()
    // }

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

/// Performs a single exact input swap
pub fn exact_input_internal<'info>(
    accounts: &mut SwapContext<'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_in: u64,
    sqrt_price_limit_x32: u64,
) -> Result<u64> {
    let pool_state = AccountLoader::<PoolState>::try_from(&accounts.pool_state)?;
    let zero_for_one = accounts.input_vault.mint == pool_state.load()?.token_0;

    let balance_before = accounts.input_vault.amount;
    swap(
        Context::new(&ID, accounts, remaining_accounts, BTreeMap::default()),
        i64::try_from(amount_in).unwrap(),
        if sqrt_price_limit_x32 == 0 {
            if zero_for_one {
                tick_math::MIN_SQRT_RATIO + 1
            } else {
                tick_math::MAX_SQRT_RATIO - 1
            }
        } else {
            sqrt_price_limit_x32
        },
    )?;

    accounts.input_vault.reload()?;
    Ok(accounts.input_vault.amount - balance_before)
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
    require!(tick >= tick_math::MIN_TICK, ErrorCode::TLM);
    require!(tick <= tick_math::MAX_TICK, ErrorCode::TUM);
    require!(tick % tick_spacing as i32 == 0, ErrorCode::TMS);
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
    require!(tick_lower < tick_upper, ErrorCode::TLU);
    Ok(())
}

/// Credit or debit liquidity to a position, and find the amount of token_0 and token_1
/// required to produce this change.
/// Returns amount of token_0 and token_1 owed to the pool, negative if the pool should
/// pay the recipient.
///
/// # Arguments
///
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Holds the initialization state of the lower tick
/// * `bitmap_upper` - Holds the initialization state of the upper tick
/// * `last_observation_state` - The last written oracle observation, having index = pool.observation_index.
/// This condition must be externally tracked.
/// * `next_observation_state` - The observation account following `last_observation_state`. Becomes equal
/// to last_observation_state when cardinality is 1.
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _modify_position<'info>(
    liquidity_delta: i64,
    pool_state: &mut PoolState,
    position_state: &AccountLoader<'info, PositionState>,
    tick_lower_state: &AccountLoader<'info, TickState>,
    tick_upper_state: &AccountLoader<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
    last_observation_state: &AccountLoader<'info, ObservationState>,
    remaining_accounts: &[AccountInfo<'info>],
) -> Result<(i64, i64)> {
    check_ticks(tick_lower_state.load()?.tick, tick_upper_state.load()?.tick)?;

    let latest_observation = last_observation_state.load()?;

    _update_position(
        liquidity_delta,
        pool_state.deref(),
        latest_observation.deref(),
        position_state,
        tick_lower_state,
        tick_upper_state,
        bitmap_lower,
        bitmap_upper,
    )?;

    let mut amount_0 = 0;
    let mut amount_1 = 0;

    let tick_lower = tick_lower_state.load()?.tick;
    let tick_upper = tick_upper_state.load()?.tick;

    if liquidity_delta != 0 {
        if pool_state.tick < tick_lower {
            // current tick is below the passed range; liquidity can only become in range by crossing from left to
            // right, when we'll need _more_ token_0 (it's becoming more valuable) so user must provide it
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        } else if pool_state.tick < tick_upper {
            // current tick is inside the passed range
            // write oracle observation
            let timestamp = oracle::_block_timestamp();
            let partition_current_timestamp = timestamp / 14;
            let partition_last_timestamp = latest_observation.block_timestamp / 14;
            drop(latest_observation);

            let next_observation_state;
            let mut new_observation = if partition_current_timestamp > partition_last_timestamp {
                next_observation_state =
                    AccountLoader::<ObservationState>::try_from(&remaining_accounts[0])?;
                let next_observation = next_observation_state.load_mut()?;
                pool_state.validate_observation_address(
                    &next_observation_state.key(),
                    next_observation.bump,
                    true,
                )?;

                next_observation
            } else {
                last_observation_state.load_mut()?
            };

            pool_state.observation_cardinality_next = new_observation.update(
                timestamp,
                pool_state.tick,
                pool_state.liquidity,
                pool_state.observation_cardinality,
                pool_state.observation_cardinality_next,
            );
            pool_state.observation_index = new_observation.index;

            // Both Δtoken_0 and Δtoken_1 will be needed in current price
            amount_0 = sqrt_price_math::get_amount_0_delta_signed(
                pool_state.sqrt_price_x32,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                pool_state.sqrt_price_x32,
                liquidity_delta,
            );

            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
        // current tick is above the range
        else {
            amount_1 = sqrt_price_math::get_amount_1_delta_signed(
                tick_math::get_sqrt_ratio_at_tick(tick_lower)?,
                tick_math::get_sqrt_ratio_at_tick(tick_upper)?,
                liquidity_delta,
            );
        }
    }

    Ok((amount_0, amount_1))
}

/// Updates a position with the given liquidity delta
///
/// # Arguments
///
/// * `pool_state` - Current pool state
/// * `position_state` - Effect change to this position
/// * `tick_lower_state`- Program account for the lower tick boundary
/// * `tick_upper_state`- Program account for the upper tick boundary
/// * `bitmap_lower` - Bitmap account for the lower tick
/// * `bitmap_upper` - Bitmap account for the upper tick, if it is different from
/// `bitmap_lower`
/// * `lamport_destination` - Destination account for freed lamports when a tick state is
/// un-initialized
/// * `liquidity_delta` - The change in liquidity. Can be 0 to perform a poke.
///
pub fn _update_position<'info>(
    liquidity_delta: i64,
    pool_state: &PoolState,
    last_observation_state: &ObservationState,
    position_state: &AccountLoader<'info, PositionState>,
    tick_lower_state: &AccountLoader<'info, TickState>,
    tick_upper_state: &AccountLoader<'info, TickState>,
    bitmap_lower: &AccountLoader<'info, TickBitmapState>,
    bitmap_upper: &AccountLoader<'info, TickBitmapState>,
) -> Result<()> {
    let mut tick_lower = tick_lower_state.load_mut()?;
    let mut tick_upper = tick_upper_state.load_mut()?;

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        let time = oracle::_block_timestamp();
        let (tick_cumulative, seconds_per_liquidity_cumulative_x32) =
            last_observation_state.observe_latest(time, pool_state.tick, pool_state.liquidity);

        let max_liquidity_per_tick =
            tick_spacing_to_max_liquidity_per_tick(pool_state.tick_spacing as i32);

        // Update tick state and find if tick is flipped
        flipped_lower = tick_lower.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0_x32,
            pool_state.fee_growth_global_1_x32,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            false,
            max_liquidity_per_tick,
        )?;
        flipped_upper = tick_upper.update(
            pool_state.tick,
            liquidity_delta,
            pool_state.fee_growth_global_0_x32,
            pool_state.fee_growth_global_1_x32,
            seconds_per_liquidity_cumulative_x32,
            tick_cumulative,
            time,
            true,
            max_liquidity_per_tick,
        )?;

        if flipped_lower {
            let bit_pos = ((tick_lower.tick / pool_state.tick_spacing as i32) % 256) as u8; // rightmost 8 bits
            bitmap_lower.load_mut()?.flip_bit(bit_pos);
        }
        if flipped_upper {
            let bit_pos = ((tick_upper.tick / pool_state.tick_spacing as i32) % 256) as u8;
            if bitmap_lower.key() == bitmap_upper.key() {
                bitmap_lower.load_mut()?.flip_bit(bit_pos);
            } else {
                bitmap_upper.load_mut()?.flip_bit(bit_pos);
            }
        }
    }
    // Update fees accrued to the position
    let (fee_growth_inside_0_x32, fee_growth_inside_1_x32) = tick::get_fee_growth_inside(
        tick_lower.deref(),
        tick_upper.deref(),
        pool_state.tick,
        pool_state.fee_growth_global_0_x32,
        pool_state.fee_growth_global_1_x32,
    );
    position_state.load_mut()?.update(
        liquidity_delta,
        fee_growth_inside_0_x32,
        fee_growth_inside_1_x32,
    )?;

    // Deallocate the tick accounts if they get un-initialized
    // A tick is un-initialized on flip if liquidity_delta is negative
    if liquidity_delta < 0 {
        if flipped_lower {
            tick_lower.clear();
        }
        if flipped_upper {
            tick_upper.clear();
        }
    }
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
        Context::new(&ID, accounts, remaining_accounts, BTreeMap::default()),
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
