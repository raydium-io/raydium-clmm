/// Oracle provides price and liquidity data useful for a wide variety of system designs
///
/// Instances of stored oracle data, "observations", are collected in the oracle array,
/// represented as PDAs with array index as seed.
///
/// Every pool is initialized with an oracle array length of 1. Anyone can pay to increase the
/// max length of the array, by initializing new accounts. New slots will be added when the
/// array is fully populated.
///
/// Observations are overwritten when the full length of the oracle array is populated.
///
/// The most recent observation is available, independent of the length of the oracle array,
/// by passing 0 as the index seed.
///
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const OBSERVATION_SEED: &str = "observation";

/// Returns data about a specific observation index
///
/// PDA of `[OBSERVATION_SEED, token_0, token_1, fee, index]`
///
#[account]
#[derive(Default, Copy)]
pub struct ObservationState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The element of the observations array stored in this account
    pub index: u16,

    /// The block timestamp of the observation
    pub block_timestamp: u32,

    /// The tick multiplied by seconds elapsed for the life of the pool as of the observation timestamp
    pub tick_cumulative: i64,

    /// The seconds per in range liquidity for the life of the pool as of the observation timestamp
    pub seconds_per_liquidity_cumulative_x32: u64,

    /// Whether the observation has been initialized and the values are safe to use
    pub initialized: bool,
}

impl ObservationState {
    /// Transforms a previous observation into a new observation, given the passage of time
    /// and the current tick and liquidity values
    ///
    /// # Arguments
    ///
    /// * `last` - Must be chronologically equal to or greater than last.blockTimestamp,
    /// safe for 0 or 1 overflows.
    /// * `block_timestamp` - The timestamp of the new observation
    /// * `tick` - The active tick at the time of the new observation
    /// * `liquidity` - The total in-range liquidity at the time of the new observation
    ///
    pub fn transform(self, block_timestamp: u32, tick: i32, liquidity: u64) -> ObservationState {
        let delta = block_timestamp - self.block_timestamp;
        ObservationState {
            bump: self.bump,
            index: self.index,
            block_timestamp,
            tick_cumulative: self.tick_cumulative + tick as i64 * delta as i64,
            seconds_per_liquidity_cumulative_x32: self.seconds_per_liquidity_cumulative_x32
                + ((delta as u64) << 32) / if liquidity > 0 { liquidity } else { 1 },
            initialized: true,
        }
    }

    /// Writes an oracle observation to the account, returning the updated cardinality.
    /// Writable at most once per second. Index represents the most recently written element.
    /// cardinality and index must be tracked externally.
    /// If the index is at the end of the allowable array length (according to cardinality),
    /// and the next cardinality is greater than the current one, cardinality may be increased.
    /// This restriction is created to preserve ordering.
    ///
    /// # Arguments
    ///
    /// * `self` - The observation account to write in
    /// * `block_timestamp` - The timestamp of the new observation
    /// * `tick` - The active tick at the time of the new observation
    /// * `liquidity` - The total in-range liquidity at the time of the new observation
    /// * `cardinality` - The number of populated elements in the oracle array
    /// * `cardinality_next` - The new length of the oracle array, independent of population
    ///
    pub fn update(
        &mut self,
        block_timestamp: u32,
        tick: i32,
        liquidity: u64,
        cardinality: u16,
        cardinality_next: u16,
    ) -> u16 {
        if self.block_timestamp == block_timestamp {
            return cardinality;
        }

        *self = self.transform(block_timestamp, tick, liquidity);

        if cardinality_next > cardinality && self.index == (cardinality - 1) {
            cardinality_next
        } else {
            cardinality
        }
    }

    /// Makes a new observation for the current block timestamp
    ///
    /// # Arguments
    ///
    /// * `last` - The most recently written observation
    /// * `time` - The current block timestamp
    /// * `liquidity` - The current in-range pool liquidity
    ///
    pub fn observe_latest(self, time: u32, tick: i32, liquidity: u64) -> (i64, u64) {
        let mut last = self;
        if self.block_timestamp != time {
            last = self.transform(time, tick, liquidity);
        }
        (
            last.tick_cumulative,
            last.seconds_per_liquidity_cumulative_x32,
        )
    }
}

/// Returns the block timestamp truncated to 32 bits, i.e. mod 2**32
///
pub fn _block_timestamp() -> u32 {
    Clock::get().unwrap().unix_timestamp as u32 // truncation is desired
}

/// Emitted by the pool for increases to the number of observations that can be stored
///
/// `observation_cardinality_next` is not the observation cardinality until an observation
/// is written at the index just before a mint/swap/burn.
///
#[event]
pub struct IncreaseObservationCardinalityNext {
    /// The previous value of the next observation cardinality
    pub observation_cardinality_next_old: u16,

    /// The updated value of the next observation cardinality
    pub observation_cardinality_next_new: u16,
}
