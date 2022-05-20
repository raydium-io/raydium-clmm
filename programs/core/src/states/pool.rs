use anchor_lang::prelude::*;

use crate::{
    program::AmmCore,
    states::{
        oracle::{self, OBSERVATION_SEED},
        position::POSITION_SEED,
        tick::TICK_SEED,
        tick_bitmap::BITMAP_SEED,
    },
};

use super::{oracle::ObservationState, tick::TickState};

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";

/// The pool state
///
/// PDA of `[POOL_SEED, token_0, token_1, fee]`
///
#[account(zero_copy)]
#[derive(Default)]
#[repr(packed)]
pub struct PoolState {
    /// Bump to identify PDA
    pub bump: u8,

    /// Token pair of the pool, where token_0 address < token_1 address
    pub token_0: Pubkey,
    pub token_1: Pubkey,

    /// Fee amount for swaps, denominated in hundredths of a bip (i.e. 1e-6)
    pub fee: u32,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,

    /// The currently in range liquidity available to the pool.
    /// This value has no relationship to the total liquidity across all ticks.
    pub liquidity: u64,

    /// The current price of the pool as a sqrt(token_1/token_0) Q32.32 value
    pub sqrt_price_x32: u64,

    /// The current tick of the pool, i.e. according to the last tick transition that was run.
    /// This value may not always be equal to SqrtTickMath.getTickAtSqrtRatio(sqrtPriceX96) if the
    /// price is on a tick boundary.
    /// Not necessarily a multiple of tick_spacing.
    pub tick: i32,

    /// the most-recently updated index of the observations array
    pub observation_index: u16,

    /// the current maximum number of observations that are being stored
    pub observation_cardinality: u16,

    /// The next maximum number of observations to store, triggered on a swap or position update
    pub observation_cardinality_next: u16,

    /// The fee growth as a Q32.32 number, i.e. fees of token_0 and token_1 collected per
    /// unit of liquidity for the entire life of the pool.
    /// These values can overflow u64
    pub fee_growth_global_0_x32: u64,
    pub fee_growth_global_1_x32: u64,

    /// The amounts of token_0 and token_1 that are owed to the protocol.
    /// Protocol fees will never exceed u64::MAX in either token
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    /// Whether the pool is currently locked to reentrancy
    pub unlocked: bool,
}

impl PoolState {
    /// Returns the observation index after the currently active one in a liquidity pool
    ///
    /// # Arguments
    /// * `self` - A pool account
    ///
    pub fn next_observation_index(self) -> u16 {
        (self.observation_index + 1) % self.observation_cardinality_next
    }

    /// Validates the public key of an observation account
    ///
    /// # Arguments
    ///
    /// * `self`- The pool to which the account belongs
    /// * `key` - The address to validated
    /// * `bump` - The PDA bump for the address
    /// * `next` - Whether to validate the current observation account or the next account
    ///
    pub fn validate_observation_address(self, key: &Pubkey, bump: u8, next: bool) -> Result<()> {
        let index = if next {
            self.next_observation_index()
        } else {
            self.observation_index
        };
        let seeds = [
            &OBSERVATION_SEED.as_bytes(),
            self.token_0.as_ref(),
            self.token_1.as_ref(),
            &self.fee.to_be_bytes(),
            &index.to_be_bytes(),
            &[bump],
        ];
        assert!(*key == Pubkey::create_program_address(&seeds, &AmmCore::id()).unwrap());
        Ok(())
    }

    /// Validates the public key of a tick account
    ///
    /// # Arguments
    ///
    /// * `self`- The pool to which the account belongs
    /// * `key` - The address to validated
    /// * `bump` - The PDA bump for the address
    /// * `tick` - The tick from which the address should be derived
    ///
    pub fn validate_tick_address(self, key: &Pubkey, bump: u8, tick: i32) -> Result<()> {
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &TICK_SEED.as_bytes(),
                    self.token_0.as_ref(),
                    self.token_1.as_ref(),
                    &self.fee.to_be_bytes(),
                    &tick.to_be_bytes(),
                    &[bump],
                ],
                &AmmCore::id(),
            )
            .unwrap(),
        );
        Ok(())
    }

    /// Validates the public key of a bitmap account
    ///
    /// # Arguments
    ///
    /// * `self`- The pool to which the account belongs
    /// * `key` - The address to validated
    /// * `bump` - The PDA bump for the address
    /// * `tick` - The tick from which the address should be derived
    ///
    pub fn validate_bitmap_address(self, key: &Pubkey, bump: u8, word_pos: i16) -> Result<()> {
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &BITMAP_SEED.as_bytes(),
                    self.token_0.as_ref(),
                    self.token_1.as_ref(),
                    &self.fee.to_be_bytes(),
                    &word_pos.to_be_bytes(),
                    &[bump],
                ],
                &AmmCore::id(),
            )
            .unwrap(),
        );
        Ok(())
    }

    /// Validates the public key of a bitmap account
    ///
    /// # Arguments
    ///
    /// * `self`- The pool to which the account belongs
    /// * `key` - The address to validated
    /// * `bump` - The PDA bump for the address
    /// * `tick` - The tick from which the address should be derived
    ///
    pub fn validate_position_address(
        self,
        key: &Pubkey,
        bump: u8,
        position_owner: &Pubkey,
        tick_lower: i32,
        tick_upper: i32,
    ) -> Result<()> {
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &POSITION_SEED.as_bytes(),
                    self.token_0.as_ref(),
                    self.token_1.as_ref(),
                    &self.fee.to_be_bytes(),
                    position_owner.as_ref(),
                    &tick_lower.to_be_bytes(),
                    &tick_upper.to_be_bytes(),
                    &[bump],
                ],
                &AmmCore::id(),
            )
            .unwrap(),
        );
        Ok(())
    }

    /// Returns a snapshot of the tick cumulative, seconds per liquidity and seconds inside a tick range
    ///
    /// Snapshots must only be compared to other snapshots, taken over a period for which a position existed.
    /// I.e., snapshots cannot be compared if a position is not held for the entire period between when the first
    /// snapshot is taken and the second snapshot is taken.
    ///
    /// # Arguments
    ///
    /// * `lower` - The lower tick of the range.
    /// * `upper` - The upper tick of the range.
    /// * `latest_observation` - The latest oracle observation. The latest condition must be externally checked.
    ///
    pub fn snapshot_cumulatives_inside(
        self,
        lower: &TickState,
        upper: &TickState,
        latest_observation: &ObservationState,
    ) -> SnapshotCumulative {
        if self.tick < lower.tick {
            SnapshotCumulative {
                tick_cumulative_inside: lower.tick_cumulative_outside
                    - upper.tick_cumulative_outside,
                seconds_per_liquidity_inside_x32: lower.seconds_per_liquidity_outside_x32
                    - upper.seconds_per_liquidity_outside_x32,
                seconds_inside: lower.seconds_outside - upper.seconds_outside,
            }
        } else if self.tick < upper.tick {
            let time = oracle::_block_timestamp();
            let ObservationState {
                tick_cumulative,
                seconds_per_liquidity_cumulative_x32,
                ..
            } = if latest_observation.block_timestamp == time {
                *latest_observation
            } else {
                latest_observation.transform(time, self.tick, self.liquidity)
            };

            SnapshotCumulative {
                tick_cumulative_inside: tick_cumulative
                    - lower.tick_cumulative_outside
                    - upper.tick_cumulative_outside,
                seconds_per_liquidity_inside_x32: seconds_per_liquidity_cumulative_x32
                    - lower.seconds_per_liquidity_outside_x32
                    - upper.seconds_per_liquidity_outside_x32,
                seconds_inside: time - lower.seconds_outside - upper.seconds_outside,
            }
        } else {
            SnapshotCumulative {
                tick_cumulative_inside: upper.tick_cumulative_outside
                    - lower.tick_cumulative_outside,
                seconds_per_liquidity_inside_x32: upper.seconds_per_liquidity_outside_x32
                    - lower.seconds_per_liquidity_outside_x32,
                seconds_inside: upper.seconds_outside - lower.seconds_outside,
            }
        }
    }
}

/// A snapshot of the tick cumulative, seconds per liquidity and seconds inside a tick range
pub struct SnapshotCumulative {
    /// The snapshot of the tick accumulator for the range.
    pub tick_cumulative_inside: i64,

    /// The snapshot of seconds per liquidity for the range.
    pub seconds_per_liquidity_inside_x32: u64,

    /// The snapshot of seconds per liquidity for the range.
    pub seconds_inside: u32,
}

/// Emitted when a pool is created and initialized with a starting price
///
#[event]
pub struct PoolCreatedAndInitialized {
    /// The first token of the pool by address sort order
    #[index]
    pub token_0: Pubkey,

    /// The second token of the pool by address sort order
    #[index]
    pub token_1: Pubkey,

    /// The fee collected upon every swap in the pool, denominated in hundredths of a bip
    #[index]
    pub fee: u32,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,

    /// The address of the created pool
    pub pool_state: Pubkey,

    /// The initial sqrt price of the pool, as a Q32.32
    pub sqrt_price_x32: u64,

    /// The initial tick of the pool, i.e. log base 1.0001 of the starting price of the pool
    pub tick: i32,
}

/// Emitted when the collected protocol fees are withdrawn by the factory owner
#[event]
pub struct CollectProtocolEvent {
    /// The pool whose protocol fee is collected
    #[index]
    pub pool_state: Pubkey,

    /// The address that collects the protocol fees
    #[index]
    pub sender: Pubkey,

    /// The address that receives the collected token_0 protocol fees
    pub recipient_wallet_0: Pubkey,

    /// The address that receives the collected token_1 protocol fees
    pub recipient_wallet_1: Pubkey,

    /// The amount of token_0 protocol fees that is withdrawn
    pub amount_0: u64,

    /// The amount of token_0 protocol fees that is withdrawn
    pub amount_1: u64,
}

/// Emitted by when a swap is performed for a pool
#[event]
pub struct SwapEvent {
    /// The pool for which token_0 and token_1 were swapped
    #[index]
    pub pool_state: Pubkey,

    /// The address that initiated the swap call, and that received the callback
    #[index]
    pub sender: Pubkey,

    /// The payer token account in zero for one swaps, or the recipient token account
    /// in one for zero swaps
    #[index]
    pub token_account_0: Pubkey,

    /// The payer token account in one for zero swaps, or the recipient token account
    /// in zero for one swaps
    #[index]
    pub token_account_1: Pubkey,

    /// The delta of the token_0 balance of the pool
    pub amount_0: i64,

    /// The delta of the token_1 balance of the pool
    pub amount_1: i64,

    /// The sqrt(price) of the pool after the swap, as a Q32.32
    pub sqrt_price_x32: u64,

    /// The liquidity of the pool after the swap
    pub liquidity: u64,

    /// The log base 1.0001 of price of the pool after the swap
    pub tick: i32,
}
