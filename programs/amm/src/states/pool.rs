use crate::error::ErrorCode;
use crate::libraries::U256;
use crate::libraries::{
    big_num::{U1024, U128},
    check_current_tick_array_is_initialized, fixed_point_64,
    full_math::MulDiv,
    next_initialized_tick_array_start_index,
};
use crate::states::{MAX_TICK_ARRAY_START_INDEX, MIN_TICK_ARRAY_START_INDEX, TICK_ARRAY_SIZE};
use anchor_lang::prelude::*;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::{BitAnd, BitOr, BitXor};

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_VAULT_SEED: &str = "pool_vault";
pub const POOL_REWARD_VAULT_SEED: &str = "pool_reward_vault";
// Number of rewards Token
pub const REWARD_NUM: usize = 3;
pub const OBSERVATION_UPDATE_DURATION_DEFAULT: u16 = 15;

pub mod reward_period_limit {
    pub const MIN_REWARD_PERIOD: u64 = 7 * 24 * 60 * 60;
    pub const MAX_REWARD_PERIOD: u64 = 90 * 24 * 60 * 60;
    pub const INCREASE_EMISSIONES_PERIOD: u64 = 72 * 60 * 60;
}

pub enum PoolStatusBitIndex {
    OpenPositionOrIncreaseLiquidity,
    DecreaseLiquidity,
    CollectFee,
    CollectReward,
    Swap,
}

#[derive(PartialEq, Eq)]
pub enum PoolStatusBitFlag {
    Enable,
    Disable,
}

/// The pool state
///
/// PDA of `[POOL_SEED, config, token_mint_0, token_mint_1]`
///
#[account(zero_copy)]
#[repr(packed)]
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
    pub mint_decimals_0: u8,
    pub mint_decimals_1: u8,

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

    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable open position and increase liquidity, 0: normal
    /// bit1, 1: disable decrease liquidity, 0: normal
    /// bit2, 1: disable collect fee, 0: normal
    /// bit3, 1: disable collect reward, 0: normal
    /// bit4, 1: disable swap, 0: normal
    pub status: u8,
    /// Leave blank for future use
    pub padding: [u8; 7],

    pub reward_infos: [RewardInfo; REWARD_NUM],

    /// Packed initialized tick array state
    pub tick_array_bitmap: [u64; 16],

    pub total_fees_token_0: u64,
    pub total_fees_claimed_token_0: u64,
    pub total_fees_token_1: u64,
    pub total_fees_claimed_token_1: u64,

    // Unused bytes for future upgrades.
    pub padding1: [u64; 28],
    pub padding2: [u64; 32],
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
        + 8 * 16
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

    pub fn pool_check_reset(&mut self, sqrt_price_x64: u128, tick: i32) -> Result<()> {
        if !U1024(self.tick_array_bitmap).is_zero() {
            return err!(ErrorCode::NotApproved);
        }
        self.sqrt_price_x64 = sqrt_price_x64;
        self.tick_current = tick;
        self.liquidity = 0;
        self.observation_index = 0;
        self.fee_growth_global_0_x64 = 0;
        self.fee_growth_global_1_x64 = 0;
        self.protocol_fees_token_0 = 0;
        self.protocol_fees_token_1 = 0;
        self.swap_in_amount_token_0 = 0;
        self.swap_out_amount_token_1 = 0;
        self.swap_in_amount_token_1 = 0;
        self.swap_out_amount_token_0 = 0;
        self.total_fees_token_0 = 0;
        self.total_fees_claimed_token_0 = 0;
        self.total_fees_token_1 = 0;
        self.total_fees_claimed_token_1 = 0;
        self.reward_infos = [RewardInfo::new(self.owner); REWARD_NUM];
        Ok(())
    }

    pub fn initialize_reward(
        &mut self,
        open_time: u64,
        end_time: u64,
        reward_per_second_x64: u128,
        token_mint: &Pubkey,
        token_vault: &Pubkey,
        authority: &Pubkey,
    ) -> Result<()> {
        let reward_infos = self.reward_infos;
        let lowest_index = match reward_infos.iter().position(|r| !r.initialized()) {
            Some(lowest_index) => lowest_index,
            None => return Err(ErrorCode::InvalidRewardIndex.into()),
        };

        for i in 0..lowest_index {
            require_keys_neq!(*token_mint, self.reward_infos[i].token_mint);
        }

        if lowest_index >= REWARD_NUM {
            return Err(ErrorCode::InvalidRewardIndex.into());
        }

        // self.reward_infos[lowest_index].reward_state = RewardState::Initialized as u8;
        self.reward_infos[lowest_index].last_update_time = open_time;
        self.reward_infos[lowest_index].open_time = open_time;
        self.reward_infos[lowest_index].end_time = end_time;
        self.reward_infos[lowest_index].emissions_per_second_x64 = reward_per_second_x64;
        self.reward_infos[lowest_index].token_mint = *token_mint;
        self.reward_infos[lowest_index].token_vault = *token_vault;
        self.reward_infos[lowest_index].authority = *authority;
        #[cfg(feature = "enable-log")]
        msg!(
            "reward_index:{}, reward_infos:{:?}",
            lowest_index,
            self.reward_infos[lowest_index],
        );
        Ok(())
    }

    // Calculates the next global reward growth variables based on the given timestamp.
    // The provided timestamp must be greater than or equal to the last updated timestamp.
    pub fn update_reward_infos(
        &mut self,
        curr_timestamp: u64,
    ) -> Result<([RewardInfo; REWARD_NUM])> {
        #[cfg(feature = "enable-log")]
        msg!("current block timestamp:{}", curr_timestamp);

        let mut next_reward_infos = self.reward_infos;

        for i in 0..REWARD_NUM {
            let reward_info = &mut next_reward_infos[i];
            if !reward_info.initialized() {
                continue;
            }
            if curr_timestamp <= reward_info.open_time {
                continue;
            }
            let latest_update_timestamp = curr_timestamp.min(reward_info.end_time);

            if self.liquidity != 0 {
                let time_delta = latest_update_timestamp - reward_info.last_update_time;

                let reward_growth_delta = U256::from(time_delta)
                    .mul_div_floor(
                        U256::from(reward_info.emissions_per_second_x64),
                        U256::from(self.liquidity),
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
                    "reward_index:{},latest_update_timestamp:{},reward_info.reward_last_update_time:{},time_delta:{},reward_emission_per_second_x64:{},reward_growth_delta:{},reward_info.reward_growth_global_x64:{}",
                    i,
                    latest_update_timestamp,
                    identity(reward_info.last_update_time),
                    time_delta,
                    identity(reward_info.emissions_per_second_x64),
                    reward_growth_delta,
                    identity(reward_info.reward_growth_global_x64)
                );
            }
            reward_info.last_update_time = latest_update_timestamp;
            // update reward state
            if latest_update_timestamp >= reward_info.open_time
                && latest_update_timestamp < reward_info.end_time
            {
                reward_info.reward_state = RewardState::Opening as u8;
            } else if latest_update_timestamp == next_reward_infos[i].end_time {
                next_reward_infos[i].reward_state = RewardState::Ended as u8;
            }
        }
        self.reward_infos = next_reward_infos;
        #[cfg(feature = "enable-log")]
        msg!("update pool reward info, reward_0_total_emissioned:{}, reward_1_total_emissioned:{},reward_2_total_emissioned:{},pool.liquidity:{}", 
        identity(self.reward_infos[0].reward_total_emissioned),identity(self.reward_infos[1].reward_total_emissioned),identity(self.reward_infos[2].reward_total_emissioned), identity(self.liquidity));
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
        let tick_array_offset_in_bitmap =
            tick_array_start_index / (self.tick_spacing as i32 * TICK_ARRAY_SIZE) + 512;
        let tick_array_bitmap = U1024(self.tick_array_bitmap);
        let mask = U1024::one() << tick_array_offset_in_bitmap.try_into().unwrap();
        self.tick_array_bitmap = tick_array_bitmap.bitxor(mask).0;
        Ok(())
    }

    /// Search the first initialized tick array from pool current tick, if current tick array is initialized then direct return,
    /// else find next according to the direction
    pub fn get_first_initialized_tick_array(&self, zero_for_one: bool) -> Option<i32> {
        let (is_initialized, start_index) = check_current_tick_array_is_initialized(
            U1024(self.tick_array_bitmap),
            self.tick_current,
            self.tick_spacing.into(),
        );
        if is_initialized {
            return Some(start_index);
        }
        next_initialized_tick_array_start_index(
            U1024(self.tick_array_bitmap),
            self.tick_current,
            self.tick_spacing.into(),
            zero_for_one,
        )
    }

    pub fn set_status(&mut self, status: u8) {
        self.status = status
    }

    pub fn set_status_by_bit(&mut self, bit: PoolStatusBitIndex, flag: PoolStatusBitFlag) {
        let s = u8::from(1) << (bit as u8);
        if flag == PoolStatusBitFlag::Disable {
            self.status = self.status.bitor(s);
        } else {
            let m = u8::from(255).bitxor(s);
            self.status = self.status.bitand(m);
        }
    }

    /// Get status by bit, if it is `noraml` status, return true
    pub fn get_status_by_bit(&self, bit: PoolStatusBitIndex) -> bool {
        let status = u8::from(1) << (bit as u8);
        self.status.bitand(status) == 0
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

#[zero_copy]
#[repr(packed)]
#[derive(Default, Debug)]
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

    pub fn get_reward_growths(reward_infos: &[RewardInfo; REWARD_NUM]) -> [u128; REWARD_NUM] {
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
    pub amount_0: u64,

    /// The delta of the token_1 balance of the pool
    pub amount_1: u64,

    /// if true, amount_0 is negtive and amount_1 is positive
    pub zero_for_one: bool,

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
        use std::convert::identity;

        #[test]
        fn get_arrary_start_index_negative() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(-600).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    9223372036854775808,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-1200).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    13835058055282163712,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-1800).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    16140901064495857664,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-38400).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    16140901064495857665,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-39000).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    9223372036854775808,
                    16140901064495857665,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-307200).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    1,
                    0,
                    0,
                    0,
                    0,
                    0,
                    9223372036854775808,
                    16140901064495857665,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            );
            pool_state.flip_tick_array_bit(-307200).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    9223372036854775808,
                    16140901064495857665,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0
                ]
            )
        }

        #[test]
        fn get_arrary_start_index_positive() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(0).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(600).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(1200).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [0, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(38400).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [0, 0, 0, 0, 0, 0, 0, 0, 7, 1, 0, 0, 0, 0, 0, 0]
            );
            pool_state.flip_tick_array_bit(306600).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    7,
                    1,
                    0,
                    0,
                    0,
                    0,
                    0,
                    9223372036854775808
                ]
            );
            pool_state.flip_tick_array_bit(306600).unwrap();
            assert_eq!(
                identity(pool_state.tick_array_bitmap),
                [0, 0, 0, 0, 0, 0, 0, 0, 7, 1, 0, 0, 0, 0, 0, 0]
            )
        }
    }

    mod poo_status_test {
        use super::*;

        #[test]
        fn get_set_status_by_bit() {
            let mut pool_state = PoolState::default();
            pool_state.set_status(17); // 00010001
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                true
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::CollectFee),
                true
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward),
                true
            );

            // disable -> disable, nothing to change
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Disable);
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );

            // disable -> enable
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Enable);
            assert_eq!(pool_state.get_status_by_bit(PoolStatusBitIndex::Swap), true);

            // enable -> enable, nothing to change
            pool_state.set_status_by_bit(
                PoolStatusBitIndex::DecreaseLiquidity,
                PoolStatusBitFlag::Enable,
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                true
            );
            // enable -> disable
            pool_state.set_status_by_bit(
                PoolStatusBitIndex::DecreaseLiquidity,
                PoolStatusBitFlag::Disable,
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                false
            );
        }
    }
}
