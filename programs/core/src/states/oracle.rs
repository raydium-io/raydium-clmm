use crate::libraries::{
    fixed_point_64,
    full_math::MulDiv,
    big_num::U128,
};
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
// Number of ObservationState element
pub const OBSERVATION_NUM: usize = 1000;

/// Returns data about a specific observation index
///
/// PDA of `[OBSERVATION_SEED, token_0, token_1, fee, index]`
///
#[account(zero_copy)]
#[repr(packed)]
pub struct ObservationState {
    /// Whether the ObservationState is initialized
    pub initialized: bool,
    pub observations: [Observation; OBSERVATION_NUM],
    /// padding for feature update
    pub padding: [u128; 5],
}
impl Default for ObservationState {
    #[inline]
    fn default() -> ObservationState {
        ObservationState {
            initialized: false,
            observations: [Observation::default(); OBSERVATION_NUM],
            padding: [0u128; 5],
        }
    }
}
/// The element of observations in ObservationState
#[account(zero_copy)]
#[repr(packed)]
#[derive(Default, Debug)]
pub struct Observation {
    /// The block timestamp of the observation
    pub block_timestamp: u32,
    /// the price of the observation timestamp, Q64.64
    pub sqrt_price_x64: u128,
    /// the cumulative of price during the duration time, Q64.64
    pub cumulative_time_price_x64: u128,
    /// padding for feature update
    pub padding: u128,
}

impl ObservationState {
    // Writes an oracle observation to the account, returning the next observation_index.
    /// Writable at most once per second. Index represents the most recently written element.
    /// cardinality and index must be tracked externally.
    /// If the index is at the end of the allowable array length (according to cardinality),
    /// and the next cardinality is greater than the current one, cardinality may be increased.
    /// This restriction is created to preserve ordering.
    ///
    /// # Arguments
    ///
    /// * `self` - The ObservationState account to write in
    /// * `block_timestamp` - The current timestamp of to update
    /// * `sqrt_price_x64` - The sqrt_price_x64 at the time of the new observation
    /// * `observation_index` - The number of populated elements in the oracle array
    /// 
    /// # Return
    /// * `next_observation_index` - The next index of the oracle array
    ///
    pub fn update_check(&mut self, block_timestamp: u32, sqrt_price_x64: u128, observation_index: u16, observation_update_duration: u32) -> Result<Option<u16>> {
        if !self.initialized {
            self.initialized = true;
            self.observations[observation_index as usize].block_timestamp = block_timestamp;
            self.observations[observation_index as usize].sqrt_price_x64 = sqrt_price_x64;
            self.observations[observation_index as usize].cumulative_time_price_x64 = 0;
            Ok(Some(observation_index))
        }
        else {
            let observation = self.observations[observation_index as usize];
            let delta_time = block_timestamp.saturating_sub(observation.block_timestamp);
            if delta_time < observation_update_duration || sqrt_price_x64 == observation.sqrt_price_x64 {
                return Ok(None);
            }
            let cur_price_x64 = U128::from(sqrt_price_x64).mul_div_floor(
                U128::from(sqrt_price_x64),
                U128::from(fixed_point_64::Q64),
            ).unwrap()
            .as_u128();
            let delta_price_x64 = cur_price_x64.checked_mul(delta_time.into()).unwrap();
            let next_observation_index = if observation_index as usize == OBSERVATION_NUM - 1 {
                0
            }
            else {
                observation_index + 1
            };
            self.observations[next_observation_index as usize].block_timestamp = block_timestamp;
            self.observations[next_observation_index as usize].sqrt_price_x64 = sqrt_price_x64;
            // cumulative_time_price_x64 may be flipped because of 'observation.cumulative_time_price_x64 + delta_price_x64' is larger than std::u128::MAX;
            // if the current observation's cumulative_time_price_x64 is smaller then the previous's,
            // the previous's real cumulative_time_price_x64 will be "cumulative_time_price_x64 + std::u128::MAX"
            self.observations[next_observation_index as usize].cumulative_time_price_x64 = observation.cumulative_time_price_x64.wrapping_add(delta_price_x64);
            Ok(Some(next_observation_index))
        }
    }
}

/// Returns the block timestamp truncated to 32 bits, i.e. mod 2**32
///
pub fn _block_timestamp() -> u32 {
    Clock::get().unwrap().unix_timestamp as u32 // truncation is desired
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::libraries::{
        get_sqrt_ratio_at_tick,
        big_num::U256,
    };
    use crate::states::pool::OBSERVATION_UPDATE_DURATION_DEFAULT;
        #[test]
    fn test_update_check_init() {
        let block_timestamp = 1647424834 as u32;
        let sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        let observation_index = 0u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
    }
    #[test]
    fn test_update_check_init_turn_around() {
        let block_timestamp = 1647424834 as u32;
        let sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        let observation_index = (OBSERVATION_NUM - 1) as u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
    }
    #[test]
    fn test_update_check_time_within_duration() {
        // init
        let mut block_timestamp = 1647424834 as u32;
        let mut sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        let mut observation_index = 0u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
        // update
        block_timestamp += 10;
        sqrt_price_x64 = get_sqrt_ratio_at_tick(1001).unwrap();
        observation_index = next_observation_index.unwrap();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == None);
    }

    #[test]
    fn test_update_check_time_out_duration_same_price() {
        // init
        let mut block_timestamp = 1647424834 as u32;
        let mut sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        let mut observation_index = 0u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
        // update
        block_timestamp += OBSERVATION_UPDATE_DURATION_DEFAULT as u32;
        sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        observation_index = next_observation_index.unwrap();
        let next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == None);
    }

    #[test]
    fn test_update_check_ok() {
        // init
        let mut block_timestamp = 1647424834 as u32;
        let mut sqrt_price_x64 = get_sqrt_ratio_at_tick(1000).unwrap();
        let mut observation_index = 0u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let mut next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
        // update
        block_timestamp += OBSERVATION_UPDATE_DURATION_DEFAULT as u32;
        sqrt_price_x64 = get_sqrt_ratio_at_tick(1001).unwrap();

        let observation = observation_state.observations[observation_index as usize];
        let delta_time = block_timestamp.saturating_sub(observation.block_timestamp);
        if delta_time < OBSERVATION_UPDATE_DURATION_DEFAULT as u32 || sqrt_price_x64 == observation.sqrt_price_x64 {
            assert!(false)
        }
        let cur_price_x64 = U128::from(sqrt_price_x64).mul_div_floor(
            U128::from(sqrt_price_x64),
            U128::from(fixed_point_64::Q64),
        ).unwrap()
        .as_u128();
        let delta_price_x64 = cur_price_x64.checked_mul(delta_time.into()).unwrap();
        let expected = observation.cumulative_time_price_x64 + delta_price_x64;
    
        observation_index = next_observation_index.unwrap();
        next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index + 1));
        observation_index = next_observation_index.unwrap();
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == expected);
    }

    #[test]
    fn test_update_check_flipped() {
        // init
        let mut block_timestamp = 1647424834 as u32;
        let mut sqrt_price_x64 = get_sqrt_ratio_at_tick(0).unwrap();
        let mut observation_index = 0u16;
        let observation_update_duration = OBSERVATION_UPDATE_DURATION_DEFAULT;
        let mut observation_state = ObservationState::default();
        let mut next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index));
        assert!(observation_state.initialized == true);
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == 0);
        observation_state.observations[observation_index as usize].cumulative_time_price_x64 = u128::max_value() - 100;
        // update
        block_timestamp += 100;
        sqrt_price_x64 = get_sqrt_ratio_at_tick(10).unwrap();

        let observation = observation_state.observations[observation_index as usize];
        let delta_time = block_timestamp.saturating_sub(observation.block_timestamp);
        if delta_time < OBSERVATION_UPDATE_DURATION_DEFAULT as u32 || sqrt_price_x64 == observation.sqrt_price_x64 {
            assert!(false)
        }
        let cur_price_x64 = U128::from(sqrt_price_x64).mul_div_floor(
            U128::from(sqrt_price_x64),
            U128::from(fixed_point_64::Q64),
        ).unwrap()
        .as_u128();
        let delta_price_x64 = cur_price_x64.checked_mul(delta_time.into()).unwrap();
        let expected = observation.cumulative_time_price_x64.wrapping_add(delta_price_x64);
        let real_expected = U256::from(observation.cumulative_time_price_x64) + U256::from(delta_price_x64);
        let expected_restore = U256::from(u128::max_value()) + U256::from(expected + 1);
        println!("delta_price_x64: {}, expected: {}, u128_max: {}", delta_price_x64, expected, u128::max_value());
        println!("real_expected: {}, expected_restore:{}", real_expected, expected_restore);

        observation_index = next_observation_index.unwrap();
        next_observation_index = observation_state.update_check(block_timestamp, sqrt_price_x64, observation_index, observation_update_duration.into()).unwrap();
        assert!(next_observation_index == Some(observation_index + 1));
        observation_index = next_observation_index.unwrap();
        assert!(observation_state.observations[observation_index as usize].block_timestamp == block_timestamp);
        assert!(observation_state.observations[observation_index as usize].sqrt_price_x64 == sqrt_price_x64);
        assert!(observation_state.observations[observation_index as usize].cumulative_time_price_x64 == expected);
    }
}
