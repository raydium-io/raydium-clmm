use crate::error::ErrorCode;
use crate::libraries::U512;
use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv};
use crate::states::{TICK_ARRAY_SIZE,MIN_TICK_ARRAY_START_INDEX,MAX_TICK_ARRAY_START_INDEX};
use anchor_lang::prelude::*;
use std::ops::{BitXor, Mul};

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_VAULT_SEED: &str = "pool_vault";
pub const POOL_REWARD_VAULT_SEED: &str = "pool_reward_vault";
// Number of rewards Token
pub const REWARD_NUM: usize = 3;
pub const OBSERVATION_UPDATE_DURATION_DEFAULT: u16 = 15;

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

    /// observation account key
    pub observation_key: Pubkey,

    /// mint0 and mint1 decimals
    pub mint_0_decimals: u8,
    pub mint_1_decimals: u8,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,
    /// The currently in range liquidity available to the pool.
    pub liquidity: u128,
    /// The current price of the pool as a sqrt(token_1/token_0) Q64.64 value
    pub sqrt_price_x64: u128,
    /// The current tick of the pool, i.e. according to the last tick transition that was run.
    pub tick_current: i32,

    /// the most-recently updated index of the observations array
    pub observation_index: u16,
    pub observation_update_duration: u16,

    /// The fee growth as a Q64.64 number, i.e. fees of token_0 and token_1 collected per
    /// unit of liquidity for the entire life of the pool.
    pub fee_growth_global_0_x64: u128,
    pub fee_growth_global_1_x64: u128,

    /// The amounts of token_0 and token_1 that are owed to the protocol.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    /// The amounts in and out of swap token_0 and token_1
    pub swap_in_amount_token_0: u128,
    pub swap_out_amount_token_1: u128,
    pub swap_in_amount_token_1: u128,
    pub swap_out_amount_token_0: u128,

    /// The lastest updated time of reward info.
    pub reward_last_updated_timestamp: u64,
    pub reward_infos: [RewardInfo; REWARD_NUM],

    /// Packed positive initialized tick array state
    pub tick_array_bitmap_positive: [u64; 8],
    /// Packed negative initialized tick array state
    pub tick_array_bitmap_negative: [u64; 8],
    // padding space for upgrade
    // pub padding_1: [u64; 16],
    // pub padding_2: [u64; 16],
    // pub padding_3: [u64; 16],
    // pub padding_4: [u64; 16],
}

impl PoolState {
    pub const LEN: usize = 8
        + 1
        + 32 * 7
        + 1
        + 1
        + 2
        + 16
        + 16
        + 4
        + 2
        + 2
        + 16
        + 16
        + 8
        + 8
        + 16
        + 16
        + 16
        + 16
        + 8
        + RewardInfo::LEN * REWARD_NUM
        + 64
        + 64
        + 512;

    pub fn key(&self) -> Pubkey {
        Pubkey::create_program_address(
            &[
                &POOL_SEED.as_bytes(),
                self.amm_config.as_ref(),
                self.token_mint_0.as_ref(),
                self.token_mint_1.as_ref(),
                &[self.bump],
            ],
            &crate::id(),
        )
        .unwrap()
    }

    pub fn initialize_reward(
        &mut self,
        curr_timestamp: u64,
        index: usize,
        open_time: u64,
        end_time: u64,
        reward_per_second_x64: u128,
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
        self.reward_infos[index].emissions_per_second_x64 = reward_per_second_x64;
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

            let reward_growth_delta = U128::from(time_delta)
                .mul_div_floor(
                    U128::from(reward_info.emissions_per_second_x64),
                    U128::from(self.liquidity),
                )
                .unwrap();

            reward_info.reward_growth_global_x64 = reward_info
                .reward_growth_global_x64
                .checked_add(reward_growth_delta.as_u128())
                .unwrap();

            reward_info.reward_total_emissioned = reward_info
                .reward_total_emissioned
                .checked_add(
                    U128::from(time_delta)
                        .mul_div_floor(
                            U128::from(reward_info.emissions_per_second_x64),
                            U128::from(fixed_point_64::Q64),
                        )
                        .unwrap()
                        .as_u64(),
                )
                .unwrap();
            #[cfg(feature = "enable-log")]
            msg!(
                "reward_index:{}, currency timestamp:{},latest_update_timestamp:{},reward_info.reward_last_update_time:{},time_delta:{},reward_emission_per_second_x64:{},reward_growth_delta:{},reward_info.reward_growth_global_x64:{}",
                i,
                curr_timestamp,
                latest_update_timestamp,
                reward_info.last_update_time,
                time_delta,
                reward_info.emissions_per_second_x64,
                reward_growth_delta,
                reward_info.reward_growth_global_x64
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

    pub fn flip_tick_array_bit(&mut self, tick_array_start_index: i32) -> Result<()> {
        require_eq!(
            0,
            tick_array_start_index % (TICK_ARRAY_SIZE * (self.tick_spacing) as i32)
        );
        assert!(tick_array_start_index >= MIN_TICK_ARRAY_START_INDEX);
        assert!(tick_array_start_index <= MAX_TICK_ARRAY_START_INDEX);
        let mut tick_array_offset_in_bitmap =
            tick_array_start_index / (self.tick_spacing as i32 * TICK_ARRAY_SIZE);
        if tick_array_start_index >= 0 {
            let tick_array_bitmap_positive = U512(self.tick_array_bitmap_positive);
            let mask = U512::from(1) << (tick_array_offset_in_bitmap as u16);
            self.tick_array_bitmap_positive = tick_array_bitmap_positive.bitxor(mask).0;
        } else {
            tick_array_offset_in_bitmap = tick_array_offset_in_bitmap.mul(-1) - 1;
            let tick_array_bitmap_negative = U512(self.tick_array_bitmap_negative);
            let mask = U512::from(1) << (tick_array_offset_in_bitmap as u16);
            self.tick_array_bitmap_negative = tick_array_bitmap_negative.bitxor(mask).0;
        }
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
    /// Q64.64 number indicates how many tokens per second are earned per unit of liquidity.
    pub emissions_per_second_x64: u128,
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
    /// Q64.64 number that tracks the total tokens earned per unit of liquidity since the reward
    /// emissions were turned on.
    pub reward_growth_global_x64: u128,
}

impl RewardInfo {
    pub const LEN: usize = 1 + 8 + 8 + 8 + 16 + 8 + 8 + 32 + 32 + 32 + 16;
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
    pub fn to_reward_growths(reward_infos: &[RewardInfo; REWARD_NUM]) -> [u128; REWARD_NUM] {
        let mut reward_growths = [0u128; REWARD_NUM];
        for i in 0..REWARD_NUM {
            reward_growths[i] = reward_infos[i].reward_growth_global_x64;
        }
        reward_growths
    }
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

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,

    /// The address of the created pool
    pub pool_state: Pubkey,

    /// The initial sqrt price of the pool, as a Q64.64
    pub sqrt_price_x64: u128,

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

    /// The sqrt(price) of the pool after the swap, as a Q64.64
    pub sqrt_price_x64: u128,

    /// The liquidity of the pool after the swap
    pub liquidity: u128,

    /// The log base 1.0001 of price of the pool after the swap
    pub tick: i32,
}

#[cfg(test)]
mod test {
    use super::*;
    mod tick_array_bitmap_test {

        use super::*;

        #[test]
        fn get_arrary_start_index_negative() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(-800).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_negative,
                [1, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(-1600).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_negative,
                [3, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(-2400).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_negative,
                [7, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(-409600).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_negative,
                [7, 0, 0, 0, 0, 0, 0, 9223372036854775808]
            );
            pool_state.flip_tick_array_bit(-409600).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_negative,
                [7, 0, 0, 0, 0, 0, 0, 0]
            )
        }

        #[test]
        fn get_arrary_start_index_positive() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(0).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_positive,
                [1, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(800).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_positive,
                [3, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(1600).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_positive,
                [7, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(408800).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_positive,
                [7, 0, 0, 0, 0, 0, 0, 9223372036854775808]
            );
            pool_state.flip_tick_array_bit(408800).unwrap();
            assert_eq!(
                pool_state.tick_array_bitmap_positive,
                [7, 0, 0, 0, 0, 0, 0, 0]
            )
        }
    }
}
