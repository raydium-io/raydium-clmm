use super::{oracle::ObservationState, tick::TickState};
use crate::error::ErrorCode;
use crate::libraries::{fixed_point_32, full_math::MulDiv};
use crate::states::{
    oracle::{self, OBSERVATION_SEED},
    position::POSITION_SEED,
    tick::TICK_SEED,
    tick_bitmap::BITMAP_SEED,
};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_VAULT_SEED: &str = "pool_vault";
pub const POOL_REWARD_VAULT_SEED: &str = "pool_reward_vault";
// Number of rewards Token
pub const REWARD_NUM: usize = 3;

/// The pool state
///
/// PDA of `[POOL_SEED, market, token_mint_0, token_mint_1, fee]`
///
#[account]
#[derive(Default, Debug)]
pub struct PoolState {
    /// Bump to identify PDA
    pub bump: u8,
    // Which config the pool belongs
    pub amm_config: Pubkey,
    // Pool creator
    pub owner: Pubkey,

    /// Token pair of the pool, where token_mint_0 address < token_mint_1 address
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,

    /// Token pair vault
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,

    /// Fee amount for swaps, denominated in hundredths of a bip (i.e. 1e-6)
    pub fee_rate: u32,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,

    /// The currently in range liquidity available to the pool.
    /// This value has no relationship to the total liquidity across all ticks.
    pub liquidity: u64,

    /// The current price of the pool as a sqrt(token_1/token_0) Q32.32 value
    pub sqrt_price: u64,

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
    pub fee_growth_global_0: u64,
    pub fee_growth_global_1: u64,

    /// The amounts of token_0 and token_1 that are owed to the protocol.
    /// Protocol fees will never exceed u64::MAX in either token
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    pub reward_last_updated_timestamp: u64,
    pub reward_infos: [RewardInfo; REWARD_NUM],
    // padding space for upgrade
    // pub padding_1: [u64; 16],
    // pub padding_2: [u64; 16],
    // pub padding_3: [u64; 16],
    // pub padding_4: [u64; 16],
}

impl PoolState {
    pub const LEN: usize =
        8 + 1 + 32 * 4 + 4 + 2 + 8 + 8 + 4 + 2 + 2 + 2 + 8 * 5 + RewardInfo::LEN * REWARD_NUM + 512;

    pub fn key(&self) -> Pubkey {
        Pubkey::create_program_address(
            &[
                &POOL_SEED.as_bytes(),
                self.token_mint_0.as_ref(),
                self.token_mint_1.as_ref(),
                &self.fee_rate.to_be_bytes(),
                &[self.bump],
            ],
            &crate::id(),
        )
        .unwrap()
    }
    /// Returns the observation index after the currently active one in a liquidity pool
    ///
    /// # Arguments
    /// * `self` - A pool account
    ///
    pub fn next_observation_index(&self) -> u16 {
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
    pub fn validate_observation_address(&self, key: &Pubkey, bump: u8, next: bool) -> Result<()> {
        let index = if next {
            self.next_observation_index()
        } else {
            self.observation_index
        };
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &OBSERVATION_SEED.as_bytes(),
                    self.key().as_ref(),
                    &index.to_be_bytes(),
                    &[bump],
                ],
                &crate::id()
            )
            .unwrap()
        );
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
    pub fn validate_tick_address(&self, key: &Pubkey, bump: u8, tick: i32) -> Result<()> {
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &TICK_SEED.as_bytes(),
                    self.key().as_ref(),
                    &tick.to_be_bytes(),
                    &[bump],
                ],
                &crate::id(),
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
    pub fn validate_bitmap_address(&self, key: &Pubkey, bump: u8, word_pos: i16) -> Result<()> {
        assert!(
            *key == Pubkey::create_program_address(
                &[
                    &BITMAP_SEED.as_bytes(),
                    self.key().as_ref(),
                    &word_pos.to_be_bytes(),
                    &[bump],
                ],
                &crate::id(),
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
        &self,
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
                    self.key().as_ref(),
                    position_owner.as_ref(),
                    &tick_lower.to_be_bytes(),
                    &tick_upper.to_be_bytes(),
                    &[bump],
                ],
                &crate::id(),
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
        &self,
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
                liquidity_cumulative: seconds_per_liquidity_cumulative_x32,
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

    pub fn initialize_reward(
        &mut self,
        curr_timestamp: u64,
        index: usize,
        open_time: u64,
        end_time: u64,
        reward_per_second_x32: u64,
        token_mint: &Pubkey,
        token_vault: &Pubkey,
    ) -> Result<()> {
        if index >= REWARD_NUM {
            return Err(ErrorCode::InvalidRewardIndex.into());
        }

        let lowest_index = match self.reward_infos.iter().position(|r| !r.initialized()) {
            Some(lowest_index) => lowest_index,
            None => return Err(ErrorCode::InvalidRewardIndex.into()),
        };

        for i in 0..lowest_index {
            require_keys_neq!(*token_mint, self.reward_infos[i].token_mint);
        }

        if lowest_index != index {
            return Err(ErrorCode::InvalidRewardIndex.into());
        }
        if open_time > curr_timestamp {
            self.reward_infos[index].reward_state = RewardState::Initialized as u8;
            self.reward_infos[index].last_update_time = open_time;
        } else {
            self.reward_infos[index].reward_state = RewardState::Opening as u8;
            self.reward_infos[index].last_update_time = curr_timestamp;
        }
        self.reward_infos[index].open_time = open_time;
        self.reward_infos[index].end_time = end_time;
        self.reward_infos[index].emissions_per_second_x32 = reward_per_second_x32;
        self.reward_infos[index].token_mint = *token_mint;
        self.reward_infos[index].token_vault = *token_vault;
        #[cfg(feature = "enable-log")]
        msg!(
            "reward_index:{},curr_timestamp:{}, reward_infos:{:?}",
            index,
            curr_timestamp,
            self.reward_infos[index],
        );
        Ok(())
    }

    // Calculates the next global reward growth variables based on the given timestamp.
    // The provided timestamp must be greater than or equal to the last updated timestamp.
    pub fn update_reward_infos(
        &mut self,
        curr_timestamp: u64,
    ) -> Result<([RewardInfo; REWARD_NUM])> {
        // No-op if no liquidity or no change in timestamp
        if self.liquidity == 0 || curr_timestamp <= self.reward_last_updated_timestamp {
            return Ok(self.reward_infos);
        }

        // Calculate new global reward growth
        let mut next_reward_infos = self.reward_infos;

        for i in 0..REWARD_NUM {
            if !next_reward_infos[i].initialized() {
                continue;
            }
            let mut latest_update_timestamp = curr_timestamp;
            if latest_update_timestamp > next_reward_infos[i].end_time {
                if next_reward_infos[i].last_update_time < next_reward_infos[i].end_time {
                    latest_update_timestamp = next_reward_infos[i].end_time
                } else {
                    continue;
                }
            }

            let reward_info = &mut next_reward_infos[i];
            let time_delta = latest_update_timestamp - reward_info.last_update_time;
            // Calculate the new reward growth delta.
            // If the calculation overflows, set the delta value to zero.
            // This will halt reward distributions for this reward.
            let reward_growth_delta = time_delta
                .mul_div_floor(reward_info.emissions_per_second_x32, self.liquidity)
                .unwrap();

            // Add the reward growth delta to the global reward growth.
            reward_info.reward_growth_global_x32 = reward_info
                .reward_growth_global_x32
                .checked_add(reward_growth_delta)
                .unwrap();

            reward_info.reward_total_emissioned = reward_info
                .reward_total_emissioned
                .checked_add(
                    time_delta
                        .mul_div_floor(reward_info.emissions_per_second_x32, fixed_point_32::Q32)
                        .unwrap(),
                )
                .unwrap();
            #[cfg(feature = "enable-log")]
            msg!(
                "reward_index:{}, currency timestamp:{},latest_update_timestamp:{},reward_info.reward_last_update_time:{},time_delta:{},reward_emission_per_second_x32:{},reward_growth_delta:{},reward_info.reward_growth_global_x32:{}",
                i,
                curr_timestamp,
                latest_update_timestamp,
                reward_info.last_update_time,
                time_delta,
                reward_info.emissions_per_second_x32,
                reward_growth_delta,
                reward_info.reward_growth_global_x32
            );
            reward_info.last_update_time = latest_update_timestamp;
        }
        self.reward_infos = next_reward_infos;
        self.reward_last_updated_timestamp = curr_timestamp;
        #[cfg(feature = "enable-log")]
        msg!("update pool reward info, reward_0_emissioned:{}, reward_1_emissioned:{},reward_2_emissioned:{},pool.liquidity:{}", 
        self.reward_infos[0].reward_total_emissioned,self.reward_infos[1].reward_total_emissioned,self.reward_infos[2].reward_total_emissioned, self.liquidity);

        Ok(next_reward_infos)
    }

    pub fn add_reward_clamed(&mut self, index: usize, amount: u64) -> Result<()> {
        assert!(index < REWARD_NUM);
        self.reward_infos[index].reward_claimed = self.reward_infos[index]
            .reward_claimed
            .checked_add(amount)
            .unwrap();
        Ok(())
    }
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Debug, PartialEq)]
/// State of reward
pub enum RewardState {
    /// Reward not initialized
    Uninitialized,
    /// Reward initialized, but reward time is not start
    Initialized,
    /// Reward in progress
    Opening,
    /// Reward end, reward time expire or
    Ended,
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Default, Debug, PartialEq)]
pub struct RewardInfo {
    /// Reward state
    pub reward_state: u8,
    /// Reward open time
    pub open_time: u64,
    /// Reward end time
    pub end_time: u64,
    /// Reward last update time
    pub last_update_time: u64,
    /// Q32.32 number indicates how many tokens per second are earned per unit of liquidity.
    pub emissions_per_second_x32: u64,
    /// The total amount of reward emissioned
    pub reward_total_emissioned: u64,
    /// The total amount of claimed reward
    pub reward_claimed: u64,
    /// Reward token mint.
    pub token_mint: Pubkey,
    /// Reward vault token account.
    pub token_vault: Pubkey,
    /// The owner that has permission to set reward param
    pub authority: Pubkey,
    /// Q32.32 number that tracks the total tokens earned per unit of liquidity since the reward
    /// emissions were turned on.
    pub reward_growth_global_x32: u64,
}

impl RewardInfo {
    pub const LEN: usize = 1 + 8 + 8 + 8 + 8 + 16 + 16 + 32 + 32 + 8; // 137
    /// Creates a new RewardInfo
    pub fn new(authority: Pubkey) -> Self {
        Self {
            authority,
            ..Default::default()
        }
    }

    /// Returns true if this reward is initialized.
    /// Once initialized, a reward cannot transition back to uninitialized.
    pub fn initialized(&self) -> bool {
        self.token_mint.ne(&Pubkey::default())
    }

    /// Maps all reward data to only the reward growth accumulators
    pub fn to_reward_growths(reward_infos: &[RewardInfo; REWARD_NUM]) -> [u64; REWARD_NUM] {
        let mut reward_growths = [0u64; REWARD_NUM];
        for i in 0..REWARD_NUM {
            reward_growths[i] = reward_infos[i].reward_growth_global_x32;
        }
        reward_growths
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
pub struct PoolCreatedEvent {
    /// The first token of the pool by address sort order
    #[index]
    pub token_mint_0: Pubkey,

    /// The second token of the pool by address sort order
    #[index]
    pub token_mint_1: Pubkey,

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

    /// Vault of token_0
    pub token_vault_0: Pubkey,
    /// Vault of token_1
    pub token_vault_1: Pubkey,
}

/// Emitted when the collected protocol fees are withdrawn by the factory owner
#[event]
pub struct CollectProtocolFeeEvent {
    /// The pool whose protocol fee is collected
    #[index]
    pub pool_state: Pubkey,

    /// The address that receives the collected token_0 protocol fees
    pub recipient_token_account_0: Pubkey,

    /// The address that receives the collected token_1 protocol fees
    pub recipient_token_account_1: Pubkey,

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
