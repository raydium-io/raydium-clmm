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
#[repr(C, packed)]
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
#[repr(C, packed)]
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

    /// Writes an oracle observation to the account
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

#[cfg(test)]
pub mod oracle_layout_test {
    use super::*;
    use anchor_lang::Discriminator;
    #[test]
    fn test_observation_layout() {
        let initialized = true;
        let recent_epoch: u64 = 0x123456789abcdef0;
        let observation_index: u16 = 0x1122;
        let pool_id: Pubkey = Pubkey::new_unique();
        let padding: [u64; 4] = [
            0x123456789abcde0f,
            0x123456789abcd0ef,
            0x123456789abc0def,
            0x123456789ab0cdef,
        ];

        let mut observation_datas = [0u8; Observation::LEN * OBSERVATION_NUM];
        let mut observations = [Observation::default(); OBSERVATION_NUM];
        let mut offset = 0;
        for i in 0..OBSERVATION_NUM {
            let index = i + 1;
            let block_timestamp: u32 = u32::MAX - 3 * index as u32;
            let tick_cumulative: i64 = i64::MAX - 3 * index as i64;
            let padding: [u64; 4] = [
                u64::MAX - index as u64,
                u64::MAX - 2 * index as u64,
                u64::MAX - 3 * index as u64,
                u64::MAX - 4 * index as u64,
            ];
            observations[i].block_timestamp = block_timestamp;
            observations[i].tick_cumulative = tick_cumulative;
            observations[i].padding = padding;
            observation_datas[offset..offset + 4].copy_from_slice(&block_timestamp.to_le_bytes());
            offset += 4;
            observation_datas[offset..offset + 8].copy_from_slice(&tick_cumulative.to_le_bytes());
            offset += 8;
            observation_datas[offset..offset + 8].copy_from_slice(&padding[0].to_le_bytes());
            offset += 8;
            observation_datas[offset..offset + 8].copy_from_slice(&padding[1].to_le_bytes());
            offset += 8;
            observation_datas[offset..offset + 8].copy_from_slice(&padding[2].to_le_bytes());
            offset += 8;
            observation_datas[offset..offset + 8].copy_from_slice(&padding[3].to_le_bytes());
            offset += 8;
        }

        // serialize original data
        let mut observation_state_data = [0u8; ObservationState::LEN];
        let mut offset = 0;
        observation_state_data[offset..offset + 8]
            .copy_from_slice(&ObservationState::discriminator());
        offset += 8;
        observation_state_data[offset..offset + 1]
            .copy_from_slice(&(initialized as u8).to_le_bytes());
        offset += 1;
        observation_state_data[offset..offset + 8].copy_from_slice(&recent_epoch.to_le_bytes());
        offset += 8;
        observation_state_data[offset..offset + 2]
            .copy_from_slice(&observation_index.to_le_bytes());
        offset += 2;
        observation_state_data[offset..offset + 32].copy_from_slice(&pool_id.to_bytes());
        offset += 32;
        observation_state_data[offset..offset + Observation::LEN * OBSERVATION_NUM]
            .copy_from_slice(&observation_datas);
        offset += Observation::LEN * OBSERVATION_NUM;
        observation_state_data[offset..offset + 8].copy_from_slice(&padding[0].to_le_bytes());
        offset += 8;
        observation_state_data[offset..offset + 8].copy_from_slice(&padding[1].to_le_bytes());
        offset += 8;
        observation_state_data[offset..offset + 8].copy_from_slice(&padding[2].to_le_bytes());
        offset += 8;
        observation_state_data[offset..offset + 8].copy_from_slice(&padding[3].to_le_bytes());
        offset += 8;
        // len check
        assert_eq!(offset, observation_state_data.len());
        assert_eq!(
            observation_state_data.len(),
            core::mem::size_of::<ObservationState>() + 8
        );

        // deserialize original data
        let unpack_data: &ObservationState = bytemuck::from_bytes(
            &observation_state_data[8..core::mem::size_of::<ObservationState>() + 8],
        );

        // data check
        let unpack_initialized = unpack_data.initialized;
        assert_eq!(unpack_initialized, initialized);
        let unpack_recent_epoch = unpack_data.recent_epoch;
        assert_eq!(unpack_recent_epoch, recent_epoch);
        let unpack_observation_index = unpack_data.observation_index;
        assert_eq!(unpack_observation_index, observation_index);
        let unpack_pool_id = unpack_data.pool_id;
        assert_eq!(unpack_pool_id, pool_id);
        let unpack_padding = unpack_data.padding;
        assert_eq!(unpack_padding, padding);
        for (observation, unpack_observation) in
            observations.iter().zip(unpack_data.observations.iter())
        {
            let block_timestamp = observation.block_timestamp;
            let tick_cumulative = observation.tick_cumulative;
            let padding = observation.padding;

            let unpack_block_timestamp = unpack_observation.block_timestamp;
            let unpack_tick_cumulative = unpack_observation.tick_cumulative;
            let unpack_padding = unpack_observation.padding;
            assert_eq!(block_timestamp, unpack_block_timestamp);
            assert_eq!(tick_cumulative, unpack_tick_cumulative);
            assert_eq!(padding, unpack_padding);
        }
    }
}
