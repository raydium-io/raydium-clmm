/// Oracle provides price data useful for a wide variety of system designs
///
use anchor_lang::prelude::*;

use crate::util::get_recent_epoch;

/// Seed to derive account address and signature
pub const OBSERVATION_SEED: &str = "observation";
// Number of ObservationState element
pub const OBSERVATION_NUM: usize = 100;
pub const OBSERVATION_UPDATE_DURATION_DEFAULT: u32 = 15;

/// The element of observations in ObservationState
#[zero_copy(unsafe)]
#[repr(packed)]
#[derive(Default, Debug)]
pub struct Observation {
    /// The block timestamp of the observation
    pub block_timestamp: u32,
    /// the cumulative of tick during the duration time
    pub tick_cumulative: i64,
    /// padding for feature update
    pub padding: [u64; 4],
}

impl Observation {
    pub const LEN: usize = 4 + 8 + 8 * 4;
}

#[account(zero_copy(unsafe))]
#[repr(packed)]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ObservationState {
    /// Whether the ObservationState is initialized
    pub initialized: bool,
    /// recent update epoch
    pub recent_epoch: u64,
    /// the most-recently updated index of the observations array
    pub observation_index: u16,
    /// belongs to which pool
    pub pool_id: Pubkey,
    /// observation array
    pub observations: [Observation; OBSERVATION_NUM],
    /// padding for feature update
    pub padding: [u64; 4],
}

impl Default for ObservationState {
    #[inline]
    fn default() -> ObservationState {
        ObservationState {
            initialized: false,
            recent_epoch: 0,
            observation_index: 0,
            pool_id: Pubkey::default(),
            observations: [Observation::default(); OBSERVATION_NUM],
            padding: [0u64; 4],
        }
    }
}

impl ObservationState {
    pub const LEN: usize = 8 + 1 + 8 + 2 + 32 + (Observation::LEN * OBSERVATION_NUM) + 8 * 4;

    pub fn initialize(&mut self, pool_id: Pubkey) -> Result<()> {
        self.initialized = false;
        self.recent_epoch = get_recent_epoch()?;
        self.observation_index = 0;
        self.pool_id = pool_id;
        self.observations = [Observation::default(); OBSERVATION_NUM];
        self.padding = [0u64; 4];
        Ok(())
    }

    // Writes an oracle observation to the account
    ///
    /// # Arguments
    ///
    /// * `self` - The ObservationState account to write in
    /// * `block_timestamp` - The current timestamp of to update
    ///
    pub fn update(&mut self, block_timestamp: u32, tick: i32) {
        let observation_index = self.observation_index;
        if !self.initialized {
            self.initialized = true;
            self.observations[observation_index as usize].block_timestamp = block_timestamp;
            self.observations[observation_index as usize].tick_cumulative = 0;
        } else {
            let last_observation = self.observations[observation_index as usize];
            let delta_time = block_timestamp.saturating_sub(last_observation.block_timestamp);
            if delta_time < OBSERVATION_UPDATE_DURATION_DEFAULT {
                return;
            }

            let delta_tick_cumulative = i64::from(tick).checked_mul(delta_time.into()).unwrap();
            let next_observation_index = if observation_index as usize == OBSERVATION_NUM - 1 {
                0
            } else {
                observation_index + 1
            };
            self.observations[next_observation_index as usize].block_timestamp = block_timestamp;
            self.observations[next_observation_index as usize].tick_cumulative = last_observation
                .tick_cumulative
                .wrapping_add(delta_tick_cumulative);
            self.observation_index = next_observation_index;
        }
    }
}

/// Returns the block timestamp truncated to 32 bits, i.e. mod 2**32
///
pub fn block_timestamp() -> u32 {
    Clock::get().unwrap().unix_timestamp as u32 // truncation is desired
}

#[cfg(test)]
pub fn block_timestamp_mock() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
