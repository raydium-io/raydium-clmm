use crate::error::ErrorCode;
use crate::libraries::{
    big_num::{U1024, U128, U256},
    check_current_tick_array_is_initialized, fixed_point_64,
    full_math::MulDiv,
    tick_array_bit_map, tick_math,
};
use crate::states::*;
use crate::util::get_recent_epoch;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::{BitAnd, BitOr, BitXor};

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_VAULT_SEED: &str = "pool_vault";
pub const POOL_REWARD_VAULT_SEED: &str = "pool_reward_vault";
pub const POOL_TICK_ARRAY_BITMAP_SEED: &str = "pool_tick_array_bitmap_extension";
// Number of rewards Token
pub const REWARD_NUM: usize = 3;

#[cfg(feature = "paramset")]
pub mod reward_period_limit {
    pub const MIN_REWARD_PERIOD: u64 = 1 * 60 * 60;
    pub const MAX_REWARD_PERIOD: u64 = 2 * 60 * 60;
    pub const INCREASE_EMISSIONES_PERIOD: u64 = 30 * 60;
}
#[cfg(not(feature = "paramset"))]
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
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct PoolState {
    /// Bump to identify PDA
    pub bump: [u8; 1],
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

    pub padding3: u16,
    pub padding4: u16,

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

    /// except protocol_fee and fund_fee
    pub total_fees_token_0: u64,
    /// except protocol_fee and fund_fee
    pub total_fees_claimed_token_0: u64,
    pub total_fees_token_1: u64,
    pub total_fees_claimed_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    // The timestamp allowed for swap in the pool.
    pub open_time: u64,
    // account recent update epoch
    pub recent_epoch: u64,

    // Unused bytes for future upgrades.
    pub padding1: [u64; 24],
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

    pub fn seeds(&self) -> [&[u8]; 5] {
        [
            &POOL_SEED.as_bytes(),
            self.amm_config.as_ref(),
            self.token_mint_0.as_ref(),
            self.token_mint_1.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn key(&self) -> Pubkey {
        Pubkey::create_program_address(&self.seeds(), &crate::id()).unwrap()
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        sqrt_price_x64: u128,
        open_time: u64,
        tick: i32,
        pool_creator: Pubkey,
        token_vault_0: Pubkey,
        token_vault_1: Pubkey,
        amm_config: &Account<AmmConfig>,
        token_mint_0: &InterfaceAccount<Mint>,
        token_mint_1: &InterfaceAccount<Mint>,
        observation_state_key: Pubkey,
    ) -> Result<()> {
        self.bump = [bump];
        self.amm_config = amm_config.key();
        self.owner = pool_creator.key();
        self.token_mint_0 = token_mint_0.key();
        self.token_mint_1 = token_mint_1.key();
        self.mint_decimals_0 = token_mint_0.decimals;
        self.mint_decimals_1 = token_mint_1.decimals;
        self.token_vault_0 = token_vault_0;
        self.token_vault_1 = token_vault_1;
        self.tick_spacing = amm_config.tick_spacing;
        self.liquidity = 0;
        self.sqrt_price_x64 = sqrt_price_x64;
        self.tick_current = tick;
        self.padding3 = 0;
        self.padding4 = 0;
        self.reward_infos = [RewardInfo::new(pool_creator); REWARD_NUM];
        self.fee_growth_global_0_x64 = 0;
        self.fee_growth_global_1_x64 = 0;
        self.protocol_fees_token_0 = 0;
        self.protocol_fees_token_1 = 0;
        self.swap_in_amount_token_0 = 0;
        self.swap_out_amount_token_1 = 0;
        self.swap_in_amount_token_1 = 0;
        self.swap_out_amount_token_0 = 0;
        self.status = 0;
        self.padding = [0; 7];
        self.tick_array_bitmap = [0; 16];
        self.total_fees_token_0 = 0;
        self.total_fees_claimed_token_0 = 0;
        self.total_fees_token_1 = 0;
        self.total_fees_claimed_token_1 = 0;
        self.fund_fees_token_0 = 0;
        self.fund_fees_token_1 = 0;
        self.open_time = open_time;
        self.recent_epoch = get_recent_epoch()?;
        self.padding1 = [0; 24];
        self.padding2 = [0; 32];
        self.observation_key = observation_state_key;

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
        operation_state: &OperationState,
    ) -> Result<()> {
        let reward_infos = self.reward_infos;
        let lowest_index = match reward_infos.iter().position(|r| !r.initialized()) {
            Some(lowest_index) => lowest_index,
            None => return Err(ErrorCode::FullRewardInfo.into()),
        };

        if lowest_index >= REWARD_NUM {
            return Err(ErrorCode::FullRewardInfo.into());
        }

        // one of first two reward token must be a vault token and the last reward token must be controled by the admin
        let reward_mints: Vec<Pubkey> = reward_infos
            .into_iter()
            .map(|item| item.token_mint)
            .collect();
        // check init token_mint is not already in use
        require!(
            !reward_mints.contains(token_mint),
            ErrorCode::RewardTokenAlreadyInUse
        );
        let whitelist_mints = operation_state.whitelist_mints.to_vec();
        // The current init token is the penult.
        if lowest_index == REWARD_NUM - 2 {
            // If token_mint_0 or token_mint_1 is not contains in the initialized rewards token,
            // the current init reward token mint must be token_mint_0 or token_mint_1
            if !reward_mints.contains(&self.token_mint_0)
                && !reward_mints.contains(&self.token_mint_1)
            {
                require!(
                    *token_mint == self.token_mint_0
                        || *token_mint == self.token_mint_1
                        || whitelist_mints.contains(token_mint),
                    ErrorCode::ExceptPoolVaultMint
                );
            }
        } else if lowest_index == REWARD_NUM - 1 {
            // the last reward token must be controled by the admin
            require!(
                *authority == crate::admin::id()
                    || operation_state.validate_operation_owner(*authority),
                ErrorCode::NotApproved
            );
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
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }

    // Calculates the next global reward growth variables based on the given timestamp.
    // The provided timestamp must be greater than or equal to the last updated timestamp.
    pub fn update_reward_infos(&mut self, curr_timestamp: u64) -> Result<[RewardInfo; REWARD_NUM]> {
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
                require_gte!(latest_update_timestamp, reward_info.last_update_time);
                let time_delta = latest_update_timestamp
                    .checked_sub(reward_info.last_update_time)
                    .unwrap();

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
                            .mul_div_ceil(
                                U128::from(reward_info.emissions_per_second_x64),
                                U128::from(fixed_point_64::Q64),
                            )
                            .unwrap()
                            .as_u64(),
                    )
                    .unwrap();
                #[cfg(feature = "enable-log")]
                msg!(
                    "reward_index:{},latest_update_timestamp:{},reward_info.reward_last_update_time:{},time_delta:{},reward_emission_per_second_x64:{},reward_growth_delta:{},reward_info.reward_growth_global_x64:{}, reward_info.reward_claim:{}",
                    i,
                    latest_update_timestamp,
                    identity(reward_info.last_update_time),
                    time_delta,
                    identity(reward_info.emissions_per_second_x64),
                    reward_growth_delta,
                    identity(reward_info.reward_growth_global_x64),
                    identity(reward_info.reward_claimed)
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
        msg!("update pool reward info, reward_0_total_emissioned:{}, reward_1_total_emissioned:{}, reward_2_total_emissioned:{}, pool.liquidity:{}",
        identity(self.reward_infos[0].reward_total_emissioned),identity(self.reward_infos[1].reward_total_emissioned),identity(self.reward_infos[2].reward_total_emissioned), identity(self.liquidity));
        self.recent_epoch = get_recent_epoch()?;
        Ok(next_reward_infos)
    }

    pub fn check_unclaimed_reward(&self, index: usize, reward_amount_owed: u64) -> Result<()> {
        assert!(index < REWARD_NUM);
        let unclaimed_reward = self.reward_infos[index]
            .reward_total_emissioned
            .checked_sub(self.reward_infos[index].reward_claimed)
            .unwrap();
        require_gte!(unclaimed_reward, reward_amount_owed);
        Ok(())
    }

    pub fn add_reward_clamed(&mut self, index: usize, amount: u64) -> Result<()> {
        assert!(index < REWARD_NUM);
        self.reward_infos[index].reward_claimed = self.reward_infos[index]
            .reward_claimed
            .checked_add(amount)
            .unwrap();
        Ok(())
    }

    pub fn get_tick_array_offset(&self, tick_array_start_index: i32) -> Result<usize> {
        require!(
            TickArrayState::check_is_valid_start_index(tick_array_start_index, self.tick_spacing),
            ErrorCode::InvaildTickIndex
        );
        let tick_array_offset_in_bitmap = tick_array_start_index
            / TickArrayState::tick_count(self.tick_spacing)
            + tick_array_bit_map::TICK_ARRAY_BITMAP_SIZE;
        Ok(tick_array_offset_in_bitmap as usize)
    }

    fn flip_tick_array_bit_internal(&mut self, tick_array_start_index: i32) -> Result<()> {
        let tick_array_offset_in_bitmap = self.get_tick_array_offset(tick_array_start_index)?;

        let tick_array_bitmap = U1024(self.tick_array_bitmap);
        let mask = U1024::one() << tick_array_offset_in_bitmap.try_into().unwrap();
        self.tick_array_bitmap = tick_array_bitmap.bitxor(mask).0;
        Ok(())
    }

    pub fn flip_tick_array_bit<'c: 'info, 'info>(
        &mut self,
        tickarray_bitmap_extension: Option<&'c AccountInfo<'info>>,
        tick_array_start_index: i32,
    ) -> Result<()> {
        if self.is_overflow_default_tickarray_bitmap(vec![tick_array_start_index]) {
            require_keys_eq!(
                tickarray_bitmap_extension.unwrap().key(),
                TickArrayBitmapExtension::key(self.key())
            );
            AccountLoader::<TickArrayBitmapExtension>::try_from(
                tickarray_bitmap_extension.unwrap(),
            )?
            .load_mut()?
            .flip_tick_array_bit(tick_array_start_index, self.tick_spacing)
        } else {
            self.flip_tick_array_bit_internal(tick_array_start_index)
        }
    }

    pub fn get_first_initialized_tick_array(
        &self,
        tickarray_bitmap_extension: &Option<TickArrayBitmapExtension>,
        zero_for_one: bool,
    ) -> Result<(bool, i32)> {
        let (is_initialized, start_index) =
            if self.is_overflow_default_tickarray_bitmap(vec![self.tick_current]) {
                tickarray_bitmap_extension
                    .unwrap()
                    .check_tick_array_is_initialized(
                        TickArrayState::get_array_start_index(self.tick_current, self.tick_spacing),
                        self.tick_spacing,
                    )?
            } else {
                check_current_tick_array_is_initialized(
                    U1024(self.tick_array_bitmap),
                    self.tick_current,
                    self.tick_spacing.into(),
                )?
            };
        if is_initialized {
            return Ok((true, start_index));
        }
        let next_start_index = self.next_initialized_tick_array_start_index(
            tickarray_bitmap_extension,
            TickArrayState::get_array_start_index(self.tick_current, self.tick_spacing),
            zero_for_one,
        )?;
        require!(
            next_start_index.is_some(),
            ErrorCode::InsufficientLiquidityForDirection
        );
        return Ok((false, next_start_index.unwrap()));
    }

    pub fn next_initialized_tick_array_start_index(
        &self,
        tickarray_bitmap_extension: &Option<TickArrayBitmapExtension>,
        mut last_tick_array_start_index: i32,
        zero_for_one: bool,
    ) -> Result<Option<i32>> {
        last_tick_array_start_index =
            TickArrayState::get_array_start_index(last_tick_array_start_index, self.tick_spacing);

        loop {
            let (is_found, start_index) =
                tick_array_bit_map::next_initialized_tick_array_start_index(
                    U1024(self.tick_array_bitmap),
                    last_tick_array_start_index,
                    self.tick_spacing,
                    zero_for_one,
                );
            if is_found {
                return Ok(Some(start_index));
            }
            last_tick_array_start_index = start_index;

            if tickarray_bitmap_extension.is_none() {
                return err!(ErrorCode::MissingTickArrayBitmapExtensionAccount);
            }

            let (is_found, start_index) = tickarray_bitmap_extension
                .unwrap()
                .next_initialized_tick_array_from_one_bitmap(
                    last_tick_array_start_index,
                    self.tick_spacing,
                    zero_for_one,
                )?;
            if is_found {
                return Ok(Some(start_index));
            }
            last_tick_array_start_index = start_index;

            if last_tick_array_start_index < tick_math::MIN_TICK
                || last_tick_array_start_index > tick_math::MAX_TICK
            {
                return Ok(None);
            }
        }
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

    pub fn is_overflow_default_tickarray_bitmap(&self, tick_indexs: Vec<i32>) -> bool {
        let (min_tick_array_start_index_boundary, max_tick_array_index_boundary) =
            self.tick_array_start_index_range();
        for tick_index in tick_indexs {
            let tick_array_start_index =
                TickArrayState::get_array_start_index(tick_index, self.tick_spacing);
            if tick_array_start_index >= max_tick_array_index_boundary
                || tick_array_start_index < min_tick_array_start_index_boundary
            {
                return true;
            }
        }
        false
    }

    // the range of tick array start index that default tickarray bitmap can represent
    // if tick_spacing = 1, the result range is [-30720, 30720)
    pub fn tick_array_start_index_range(&self) -> (i32, i32) {
        // the range of ticks that default tickarrary can represent
        let mut max_tick_boundary =
            tick_array_bit_map::max_tick_in_tickarray_bitmap(self.tick_spacing);
        let mut min_tick_boundary = -max_tick_boundary;
        if max_tick_boundary > tick_math::MAX_TICK {
            max_tick_boundary =
                TickArrayState::get_array_start_index(tick_math::MAX_TICK, self.tick_spacing);
            // find the next tick array start index
            max_tick_boundary = max_tick_boundary + TickArrayState::tick_count(self.tick_spacing);
        }
        if min_tick_boundary < tick_math::MIN_TICK {
            min_tick_boundary =
                TickArrayState::get_array_start_index(tick_math::MIN_TICK, self.tick_spacing);
        }
        (min_tick_boundary, max_tick_boundary)
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

#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug, PartialEq, Eq)]
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
#[cfg_attr(feature = "client", derive(Debug))]
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
#[cfg_attr(feature = "client", derive(Debug))]
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
#[cfg_attr(feature = "client", derive(Debug))]
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

    /// The real delta amount of the token_0 of the pool or user
    pub amount_0: u64,

    /// The transfer fee charged by the withheld_amount of the token_0
    pub transfer_fee_0: u64,

    /// The real delta of the token_1 of the pool or user
    pub amount_1: u64,

    /// The transfer fee charged by the withheld_amount of the token_1
    pub transfer_fee_1: u64,

    /// if true, amount_0 is negtive and amount_1 is positive
    pub zero_for_one: bool,

    /// The sqrt(price) of the pool after the swap, as a Q64.64
    pub sqrt_price_x64: u128,

    /// The liquidity of the pool after the swap
    pub liquidity: u128,

    /// The log base 1.0001 of price of the pool after the swap
    pub tick: i32,
}

/// Emitted pool liquidity change when increase and decrease liquidity
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LiquidityChangeEvent {
    /// The pool for swap
    #[index]
    pub pool_state: Pubkey,

    /// The tick of the pool
    pub tick: i32,

    /// The tick lower of position
    pub tick_lower: i32,

    /// The tick lower of position
    pub tick_upper: i32,

    /// The liquidity of the pool before liquidity change
    pub liquidity_before: u128,

    /// The liquidity of the pool after liquidity change
    pub liquidity_after: u128,
}

// /// Emitted when price move in a swap step
// #[event]
// #[cfg_attr(feature = "client", derive(Debug))]
// pub struct PriceChangeEvent {
//     /// The pool for swap
//     #[index]
//     pub pool_state: Pubkey,

//     /// The tick of the pool before price change
//     pub tick_before: i32,

//     /// The tick of the pool after tprice change
//     pub tick_after: i32,

//     /// The sqrt(price) of the pool before price change, as a Q64.64
//     pub sqrt_price_x64_before: u128,

//     /// The sqrt(price) of the pool after price change, as a Q64.64
//     pub sqrt_price_x64_after: u128,

//     /// The liquidity of the pool before price change
//     pub liquidity_before: u128,

//     /// The liquidity of the pool after price change
//     pub liquidity_after: u128,

//     /// The direction of swap
//     pub zero_for_one: bool,
// }

#[cfg(test)]
pub mod pool_test {
    use super::*;
    use std::cell::RefCell;

    pub fn build_pool(
        tick_current: i32,
        tick_spacing: u16,
        sqrt_price_x64: u128,
        liquidity: u128,
    ) -> RefCell<PoolState> {
        let mut new_pool = PoolState::default();
        new_pool.tick_current = tick_current;
        new_pool.tick_spacing = tick_spacing;
        new_pool.sqrt_price_x64 = sqrt_price_x64;
        new_pool.liquidity = liquidity;
        new_pool.token_mint_0 = Pubkey::new_unique();
        new_pool.token_mint_1 = Pubkey::new_unique();
        new_pool.amm_config = Pubkey::new_unique();
        // let mut random = rand::random<u128>();
        new_pool.fee_growth_global_0_x64 = rand::random::<u128>();
        new_pool.fee_growth_global_1_x64 = rand::random::<u128>();
        new_pool.bump = [Pubkey::find_program_address(
            &[
                &POOL_SEED.as_bytes(),
                new_pool.amm_config.as_ref(),
                new_pool.token_mint_0.as_ref(),
                new_pool.token_mint_1.as_ref(),
            ],
            &crate::id(),
        )
        .1];
        RefCell::new(new_pool)
    }

    mod tick_array_bitmap_test {

        use super::*;

        #[test]
        fn get_arrary_start_index_negative() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(None, -600).unwrap();
            assert!(U1024(pool_state.tick_array_bitmap).bit(511) == true);

            pool_state.flip_tick_array_bit(None, -1200).unwrap();
            assert!(U1024(pool_state.tick_array_bitmap).bit(510) == true);

            pool_state.flip_tick_array_bit(None, -1800).unwrap();
            assert!(U1024(pool_state.tick_array_bitmap).bit(509) == true);

            pool_state.flip_tick_array_bit(None, -38400).unwrap();
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(-38400).unwrap())
                    == true
            );
            pool_state.flip_tick_array_bit(None, -39000).unwrap();
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(-39000).unwrap())
                    == true
            );
            pool_state.flip_tick_array_bit(None, -307200).unwrap();
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(-307200).unwrap())
                    == true
            );
        }

        #[test]
        fn get_arrary_start_index_positive() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 10;
            pool_state.flip_tick_array_bit(None, 0).unwrap();
            assert!(pool_state.get_tick_array_offset(0).unwrap() == 512);
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(0).unwrap())
                    == true
            );

            pool_state.flip_tick_array_bit(None, 600).unwrap();
            assert!(pool_state.get_tick_array_offset(600).unwrap() == 513);
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(600).unwrap())
                    == true
            );

            pool_state.flip_tick_array_bit(None, 1200).unwrap();
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(1200).unwrap())
                    == true
            );

            pool_state.flip_tick_array_bit(None, 38400).unwrap();
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(38400).unwrap())
                    == true
            );

            pool_state.flip_tick_array_bit(None, 306600).unwrap();
            assert!(pool_state.get_tick_array_offset(306600).unwrap() == 1023);
            assert!(
                U1024(pool_state.tick_array_bitmap)
                    .bit(pool_state.get_tick_array_offset(306600).unwrap())
                    == true
            );
        }

        #[test]
        fn default_tick_array_start_index_range_test() {
            let mut pool_state = PoolState::default();
            pool_state.tick_spacing = 60;
            // -443580 is the min tick can use to open a position when tick_spacing is 60 due to MIN_TICK is -443636
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![-443580]) == false);
            // 443580 is the min tick can use to open a position when tick_spacing is 60 due to MAX_TICK is 443636
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![443580]) == false);

            pool_state.tick_spacing = 10;
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![-307200]) == false);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![-307201]) == true);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![307200]) == true);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![307199]) == false);

            pool_state.tick_spacing = 1;
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![-30720]) == false);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![-30721]) == true);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![30720]) == true);
            assert!(pool_state.is_overflow_default_tickarray_bitmap(vec![30719]) == false);
        }
    }

    mod pool_status_test {
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

    mod update_reward_infos_test {
        use super::*;
        use anchor_lang::prelude::Pubkey;
        use std::convert::identity;
        use std::str::FromStr;

        #[test]
        fn reward_info_test() {
            let pool_state = &mut PoolState::default();
            let operation_state = OperationState {
                bump: 0,
                operation_owners: [Pubkey::default(); OPERATION_SIZE_USIZE],
                whitelist_mints: [Pubkey::default(); WHITE_MINT_SIZE_USIZE],
            };
            pool_state
                .initialize_reward(
                    1665982800,
                    1666069200,
                    10,
                    &Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap(),
                    &Pubkey::default(),
                    &Pubkey::default(),
                    &operation_state,
                )
                .unwrap();

            // before start time, nothing to update
            let mut updated_reward_infos = pool_state.update_reward_infos(1665982700).unwrap();
            assert_eq!(updated_reward_infos[0], pool_state.reward_infos[0]);

            // pool liquidity is 0
            updated_reward_infos = pool_state.update_reward_infos(1665982900).unwrap();
            assert_eq!(
                identity(updated_reward_infos[0].reward_growth_global_x64),
                0
            );

            pool_state.liquidity = 100;
            updated_reward_infos = pool_state.update_reward_infos(1665983000).unwrap();
            assert_eq!(
                identity(updated_reward_infos[0].last_update_time),
                1665983000
            );
            assert_eq!(
                identity(updated_reward_infos[0].reward_growth_global_x64),
                10
            );

            // curr_timestamp grater than reward end time
            updated_reward_infos = pool_state.update_reward_infos(1666069300).unwrap();
            assert_eq!(
                identity(updated_reward_infos[0].last_update_time),
                1666069200
            );
        }
    }

    mod use_tickarray_bitmap_extension_test {

        use std::ops::Deref;

        use super::*;

        use crate::tick_array_bitmap_extension_test::{
            build_tick_array_bitmap_extension_info, BuildExtensionAccountInfo,
        };

        pub fn pool_flip_tick_array_bit_helper<'c: 'info, 'info>(
            pool_state: &mut PoolState,
            tickarray_bitmap_extension: Option<&'c AccountInfo<'info>>,
            init_tick_array_start_indexs: Vec<i32>,
        ) {
            for start_index in init_tick_array_start_indexs {
                pool_state
                    .flip_tick_array_bit(tickarray_bitmap_extension, start_index)
                    .unwrap();
            }
        }

        #[test]
        fn get_first_initialized_tick_array_test() {
            let tick_spacing = 1;
            let tick_current = tick_spacing * TICK_ARRAY_SIZE * 511 - 1;

            let pool_state_refcel = build_pool(
                tick_current,
                tick_spacing.try_into().unwrap(),
                tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                0,
            );

            let mut pool_state = pool_state_refcel.borrow_mut();

            let param: &mut BuildExtensionAccountInfo = &mut BuildExtensionAccountInfo::default();
            param.key = Pubkey::find_program_address(
                &[
                    POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                    pool_state.key().as_ref(),
                ],
                &crate::id(),
            )
            .0;
            let tick_array_bitmap_extension_info: AccountInfo<'_> =
                build_tick_array_bitmap_extension_info(param);

            pool_flip_tick_array_bit_helper(
                &mut pool_state,
                Some(&tick_array_bitmap_extension_info),
                vec![
                    -tick_spacing * TICK_ARRAY_SIZE * 513, // tick in extension
                    tick_spacing * TICK_ARRAY_SIZE * 511,
                    tick_spacing * TICK_ARRAY_SIZE * 512, // tick in extension
                ],
            );

            let tick_array_bitmap_extension = Some(
                *AccountLoader::<TickArrayBitmapExtension>::try_from(
                    &tick_array_bitmap_extension_info,
                )
                .unwrap()
                .load()
                .unwrap()
                .deref(),
            );

            let (is_first_initilzied, start_index) = pool_state
                .get_first_initialized_tick_array(&tick_array_bitmap_extension, true)
                .unwrap();
            assert!(is_first_initilzied == false);
            assert!(start_index == -tick_spacing * TICK_ARRAY_SIZE * 513);

            let (is_first_initilzied, start_index) = pool_state
                .get_first_initialized_tick_array(&tick_array_bitmap_extension, false)
                .unwrap();
            assert!(is_first_initilzied == false);
            assert!(start_index == tick_spacing * TICK_ARRAY_SIZE * 511);

            pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 511;
            let (is_first_initilzied, start_index) = pool_state
                .get_first_initialized_tick_array(&tick_array_bitmap_extension, true)
                .unwrap();
            assert!(is_first_initilzied == true);
            assert!(start_index == tick_spacing * TICK_ARRAY_SIZE * 511);

            pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 512;
            let (is_first_initilzied, start_index) = pool_state
                .get_first_initialized_tick_array(&tick_array_bitmap_extension, true)
                .unwrap();
            assert!(is_first_initilzied == true);
            assert!(start_index == tick_spacing * TICK_ARRAY_SIZE * 512);
        }

        mod next_initialized_tick_array_start_index_test {

            use super::*;
            #[test]
            fn from_pool_bitmap_to_extension_negative_bitmap() {
                let tick_spacing = 1;
                let tick_current = tick_spacing * TICK_ARRAY_SIZE * 511;

                let pool_state_refcel = build_pool(
                    tick_current,
                    tick_spacing.try_into().unwrap(),
                    tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                    0,
                );

                let mut pool_state = pool_state_refcel.borrow_mut();

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                param.key = Pubkey::find_program_address(
                    &[
                        POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                        pool_state.key().as_ref(),
                    ],
                    &crate::id(),
                )
                .0;

                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![
                        -tick_spacing * TICK_ARRAY_SIZE * 7394, // max negative tick array start index boundary in extension
                        -tick_spacing * TICK_ARRAY_SIZE * 1000, // tick in extension
                        -tick_spacing * TICK_ARRAY_SIZE * 513,  // tick in extension
                        tick_spacing * TICK_ARRAY_SIZE * 510,   // tick in pool bitmap
                    ],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert_eq!(start_index.unwrap(), tick_spacing * TICK_ARRAY_SIZE * 510);

                pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 510;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == -tick_spacing * TICK_ARRAY_SIZE * 513);

                pool_state.tick_current = -tick_spacing * TICK_ARRAY_SIZE * 513;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == -tick_spacing * TICK_ARRAY_SIZE * 1000);

                pool_state.tick_current = -tick_spacing * TICK_ARRAY_SIZE * 7393;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == -tick_spacing * TICK_ARRAY_SIZE * 7394);

                pool_state.tick_current = -tick_spacing * TICK_ARRAY_SIZE * 7394;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.is_none() == true);
            }

            #[test]
            fn from_pool_bitmap_to_extension_positive_bitmap() {
                let tick_spacing = 1;
                let tick_current = 0;

                let pool_state_refcel = build_pool(
                    tick_current,
                    tick_spacing.try_into().unwrap(),
                    tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                    0,
                );

                let mut pool_state = pool_state_refcel.borrow_mut();

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                param.key = Pubkey::find_program_address(
                    &[
                        POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                        pool_state.key().as_ref(),
                    ],
                    &crate::id(),
                )
                .0;
                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![
                        tick_spacing * TICK_ARRAY_SIZE * 510,  // tick in pool bitmap
                        tick_spacing * TICK_ARRAY_SIZE * 511,  // tick in pool bitmap
                        tick_spacing * TICK_ARRAY_SIZE * 512,  // tick in extension boundary
                        tick_spacing * TICK_ARRAY_SIZE * 7393, // max positvie tick array start index boundary in extension
                    ],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == tick_spacing * TICK_ARRAY_SIZE * 510);

                pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 510;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == tick_spacing * TICK_ARRAY_SIZE * 511);

                pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 511;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == tick_spacing * TICK_ARRAY_SIZE * 512);

                pool_state.tick_current = tick_spacing * TICK_ARRAY_SIZE * 7393;
                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.is_none() == true);
            }

            #[test]
            fn from_extension_negative_bitmap_to_extension_positive_bitmap() {
                let tick_spacing = 1;
                let tick_current = -tick_spacing * TICK_ARRAY_SIZE * 999;

                let pool_state_refcel = build_pool(
                    tick_current,
                    tick_spacing.try_into().unwrap(),
                    tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                    0,
                );

                let mut pool_state = pool_state_refcel.borrow_mut();

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                param.key = Pubkey::find_program_address(
                    &[
                        POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                        pool_state.key().as_ref(),
                    ],
                    &crate::id(),
                )
                .0;

                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![
                        -tick_spacing * TICK_ARRAY_SIZE * 1000, // tick in extension
                        tick_spacing * TICK_ARRAY_SIZE * 512,   // tick in extension boundary
                        tick_spacing * TICK_ARRAY_SIZE * 1000,  // tick in extension
                    ],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == tick_spacing * TICK_ARRAY_SIZE * 512);
            }

            #[test]
            fn from_extension_positive_bitmap_to_extension_negative_bitmap() {
                let tick_spacing = 1;
                let tick_current = tick_spacing * TICK_ARRAY_SIZE * 999;

                let pool_state_refcel = build_pool(
                    tick_current,
                    tick_spacing.try_into().unwrap(),
                    tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                    0,
                );

                let mut pool_state = pool_state_refcel.borrow_mut();

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                param.key = Pubkey::find_program_address(
                    &[
                        POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                        pool_state.key().as_ref(),
                    ],
                    &crate::id(),
                )
                .0;
                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![
                        -tick_spacing * TICK_ARRAY_SIZE * 1000, // tick in extension
                        -tick_spacing * TICK_ARRAY_SIZE * 513,  // tick in extension
                        tick_spacing * TICK_ARRAY_SIZE * 1000,  // tick in extension
                    ],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == -tick_spacing * TICK_ARRAY_SIZE * 513);
            }

            #[test]
            fn no_initialized_tick_array() {
                let mut pool_state = PoolState::default();
                pool_state.tick_spacing = 1;
                pool_state.tick_current = 0;

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        true,
                    )
                    .unwrap();
                assert!(start_index.is_none());

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        pool_state.tick_current,
                        false,
                    )
                    .unwrap();
                assert!(start_index.is_none());
            }

            #[test]
            fn min_tick_max_tick_initialized_test() {
                let tick_spacing = 1;
                let tick_current = 0;

                let pool_state_refcel = build_pool(
                    tick_current,
                    tick_spacing.try_into().unwrap(),
                    tick_math::get_sqrt_price_at_tick(tick_current).unwrap(),
                    0,
                );

                let mut pool_state = pool_state_refcel.borrow_mut();

                let param: &mut BuildExtensionAccountInfo =
                    &mut BuildExtensionAccountInfo::default();
                param.key = Pubkey::find_program_address(
                    &[
                        POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
                        pool_state.key().as_ref(),
                    ],
                    &crate::id(),
                )
                .0;
                let tick_array_bitmap_extension_info: AccountInfo<'_> =
                    build_tick_array_bitmap_extension_info(param);

                pool_flip_tick_array_bit_helper(
                    &mut pool_state,
                    Some(&tick_array_bitmap_extension_info),
                    vec![
                        -tick_spacing * TICK_ARRAY_SIZE * 7394, // The tickarray where min_tick(-443636) is located
                        tick_spacing * TICK_ARRAY_SIZE * 7393, // The tickarray where max_tick(443636) is located
                    ],
                );

                let tick_array_bitmap_extension = Some(
                    *AccountLoader::<TickArrayBitmapExtension>::try_from(
                        &tick_array_bitmap_extension_info,
                    )
                    .unwrap()
                    .load()
                    .unwrap()
                    .deref(),
                );

                let start_index = pool_state
                    .next_initialized_tick_array_start_index(
                        &tick_array_bitmap_extension,
                        -tick_spacing * TICK_ARRAY_SIZE * 7394,
                        false,
                    )
                    .unwrap();
                assert!(start_index.unwrap() == tick_spacing * TICK_ARRAY_SIZE * 7393);
            }
        }
    }

    mod pool_layout_test {
        use super::*;
        use anchor_lang::Discriminator;
        #[test]
        fn test_pool_layout() {
            let bump: u8 = 0x12;
            let amm_config = Pubkey::new_unique();
            let owner = Pubkey::new_unique();
            let token_mint_0 = Pubkey::new_unique();
            let token_mint_1 = Pubkey::new_unique();
            let token_vault_0 = Pubkey::new_unique();
            let token_vault_1 = Pubkey::new_unique();
            let observation_key = Pubkey::new_unique();
            let mint_decimals_0: u8 = 0x13;
            let mint_decimals_1: u8 = 0x14;
            let tick_spacing: u16 = 0x1516;
            let liquidity: u128 = 0x11002233445566778899aabbccddeeff;
            let sqrt_price_x64: u128 = 0x11220033445566778899aabbccddeeff;
            let tick_current: i32 = 0x12345678;
            let padding3: u16 = 0x1718;
            let padding4: u16 = 0x191a;
            let fee_growth_global_0_x64: u128 = 0x11223300445566778899aabbccddeeff;
            let fee_growth_global_1_x64: u128 = 0x11223344005566778899aabbccddeeff;
            let protocol_fees_token_0: u64 = 0x123456789abcdef0;
            let protocol_fees_token_1: u64 = 0x123456789abcde0f;
            let swap_in_amount_token_0: u128 = 0x11223344550066778899aabbccddeeff;
            let swap_out_amount_token_1: u128 = 0x11223344556600778899aabbccddeeff;
            let swap_in_amount_token_1: u128 = 0x11223344556677008899aabbccddeeff;
            let swap_out_amount_token_0: u128 = 0x11223344556677880099aabbccddeeff;
            let status: u8 = 0x1b;
            let padding: [u8; 7] = [0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18];
            // RewardInfo
            let reward_state: u8 = 0x1c;
            let open_time: u64 = 0x123456789abc0def;
            let end_time: u64 = 0x123456789ab0cdef;
            let last_update_time: u64 = 0x123456789a0bcdef;
            let emissions_per_second_x64: u128 = 0x11223344556677889900aabbccddeeff;
            let reward_total_emissioned: u64 = 0x1234567890abcdef;
            let reward_claimed: u64 = 0x1234567809abcdef;
            let token_mint = Pubkey::new_unique();
            let token_vault = Pubkey::new_unique();
            let authority = Pubkey::new_unique();
            let reward_growth_global_x64: u128 = 0x112233445566778899aa00bbccddeeff;
            let mut reward_info_data = [0u8; RewardInfo::LEN];

            let mut offset = 0;
            reward_info_data[offset..offset + 1].copy_from_slice(&reward_state.to_le_bytes());
            offset += 1;
            reward_info_data[offset..offset + 8].copy_from_slice(&open_time.to_le_bytes());
            offset += 8;
            reward_info_data[offset..offset + 8].copy_from_slice(&end_time.to_le_bytes());
            offset += 8;
            reward_info_data[offset..offset + 8].copy_from_slice(&last_update_time.to_le_bytes());
            offset += 8;
            reward_info_data[offset..offset + 16]
                .copy_from_slice(&emissions_per_second_x64.to_le_bytes());
            offset += 16;
            reward_info_data[offset..offset + 8]
                .copy_from_slice(&reward_total_emissioned.to_le_bytes());
            offset += 8;
            reward_info_data[offset..offset + 8].copy_from_slice(&reward_claimed.to_le_bytes());
            offset += 8;
            reward_info_data[offset..offset + 32].copy_from_slice(&token_mint.to_bytes());
            offset += 32;
            reward_info_data[offset..offset + 32].copy_from_slice(&token_vault.to_bytes());
            offset += 32;
            reward_info_data[offset..offset + 32].copy_from_slice(&authority.to_bytes());
            offset += 32;
            reward_info_data[offset..offset + 16]
                .copy_from_slice(&reward_growth_global_x64.to_le_bytes());
            let mut reward_info_datas = [0u8; RewardInfo::LEN * REWARD_NUM];
            let mut offset = 0;
            for _ in 0..REWARD_NUM {
                reward_info_datas[offset..offset + RewardInfo::LEN]
                    .copy_from_slice(&reward_info_data);
                offset += RewardInfo::LEN;
            }
            assert_eq!(offset, reward_info_datas.len());
            assert_eq!(
                reward_info_datas.len(),
                core::mem::size_of::<RewardInfo>() * 3
            );

            // tick_array_bitmap
            let mut tick_array_bitmap: [u64; 16] = [0u64; 16];
            let mut tick_array_bitmap_data = [0u8; 8 * 16];
            let mut offset = 0;
            for i in 0..16 {
                tick_array_bitmap[i] = u64::MAX << i;
                tick_array_bitmap_data[offset..offset + 8]
                    .copy_from_slice(&tick_array_bitmap[i].to_le_bytes());
                offset += 8;
            }
            let total_fees_token_0: u64 = 0x1234567809abcdef;
            let total_fees_token_1: u64 = 0x1234567089abcdef;
            let total_fees_claimed_token_0: u64 = 0x1234560789abcdef;
            let total_fees_claimed_token_1: u64 = 0x1234506789abcdef;
            let fund_fees_token_0: u64 = 0x1234056789abcdef;
            let fund_fees_token_1: u64 = 0x1230456789abcdef;
            let pool_open_time: u64 = 0x1203456789abcdef;
            let recent_epoch: u64 = 0x1023456789abcdef;
            let mut padding1: [u64; 24] = [0u64; 24];
            let mut padding1_data = [0u8; 8 * 24];
            let mut offset = 0;
            for i in 0..24 {
                padding1[i] = u64::MAX - i as u64;
                padding1_data[offset..offset + 8].copy_from_slice(&padding1[i].to_le_bytes());
                offset += 8;
            }
            let mut padding2: [u64; 32] = [0u64; 32];
            let mut padding2_data = [0u8; 8 * 32];
            let mut offset = 0;
            for i in 24..(24 + 32) {
                padding2[i - 24] = u64::MAX - i as u64;
                padding2_data[offset..offset + 8].copy_from_slice(&padding2[i - 24].to_le_bytes());
                offset += 8;
            }
            // serialize original data
            let mut pool_data = [0u8; PoolState::LEN];
            let mut offset = 0;
            pool_data[offset..offset + 8].copy_from_slice(&PoolState::discriminator());
            offset += 8;
            pool_data[offset..offset + 1].copy_from_slice(&bump.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 32].copy_from_slice(&amm_config.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&owner.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_mint_0.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_mint_1.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_vault_0.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_vault_1.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&observation_key.to_bytes());
            offset += 32;
            pool_data[offset..offset + 1].copy_from_slice(&mint_decimals_0.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 1].copy_from_slice(&mint_decimals_1.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 2].copy_from_slice(&tick_spacing.to_le_bytes());
            offset += 2;
            pool_data[offset..offset + 16].copy_from_slice(&liquidity.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&sqrt_price_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 4].copy_from_slice(&tick_current.to_le_bytes());
            offset += 4;
            pool_data[offset..offset + 2].copy_from_slice(&padding3.to_le_bytes());
            offset += 2;
            pool_data[offset..offset + 2].copy_from_slice(&padding4.to_le_bytes());
            offset += 2;
            pool_data[offset..offset + 16].copy_from_slice(&fee_growth_global_0_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&fee_growth_global_1_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 8].copy_from_slice(&protocol_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&protocol_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 16].copy_from_slice(&swap_in_amount_token_0.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_out_amount_token_1.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_in_amount_token_1.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_out_amount_token_0.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 1].copy_from_slice(&status.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 7].copy_from_slice(&padding);
            offset += 7;
            pool_data[offset..offset + RewardInfo::LEN * REWARD_NUM]
                .copy_from_slice(&reward_info_datas);
            offset += RewardInfo::LEN * REWARD_NUM;
            pool_data[offset..offset + 8 * 16].copy_from_slice(&tick_array_bitmap_data);
            offset += 8 * 16;
            pool_data[offset..offset + 8].copy_from_slice(&total_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8]
                .copy_from_slice(&total_fees_claimed_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&total_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8]
                .copy_from_slice(&total_fees_claimed_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&fund_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&fund_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&pool_open_time.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&recent_epoch.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8 * 24].copy_from_slice(&padding1_data);
            offset += 8 * 24;
            pool_data[offset..offset + 8 * 32].copy_from_slice(&padding2_data);
            offset += 8 * 32;

            // len check
            assert_eq!(offset, pool_data.len());
            assert_eq!(pool_data.len(), core::mem::size_of::<PoolState>() + 8);

            // deserialize original data
            let unpack_data: &PoolState =
                bytemuck::from_bytes(&pool_data[8..core::mem::size_of::<PoolState>() + 8]);

            // data check
            let unpack_bump = unpack_data.bump[0];
            assert_eq!(unpack_bump, bump);
            let unpack_amm_config = unpack_data.amm_config;
            assert_eq!(unpack_amm_config, amm_config);
            let unpack_owner = unpack_data.owner;
            assert_eq!(unpack_owner, owner);
            let unpack_token_mint_0 = unpack_data.token_mint_0;
            assert_eq!(unpack_token_mint_0, token_mint_0);
            let unpack_token_mint_1 = unpack_data.token_mint_1;
            assert_eq!(unpack_token_mint_1, token_mint_1);
            let unpack_token_vault_0 = unpack_data.token_vault_0;
            assert_eq!(unpack_token_vault_0, token_vault_0);
            let unpack_token_vault_1 = unpack_data.token_vault_1;
            assert_eq!(unpack_token_vault_1, token_vault_1);
            let unpack_observation_key = unpack_data.observation_key;
            assert_eq!(unpack_observation_key, observation_key);
            let unpack_mint_decimals_0 = unpack_data.mint_decimals_0;
            assert_eq!(unpack_mint_decimals_0, mint_decimals_0);
            let unpack_mint_decimals_1 = unpack_data.mint_decimals_1;
            assert_eq!(unpack_mint_decimals_1, mint_decimals_1);
            let unpack_tick_spacing = unpack_data.tick_spacing;
            assert_eq!(unpack_tick_spacing, tick_spacing);
            let unpack_liquidity = unpack_data.liquidity;
            assert_eq!(unpack_liquidity, liquidity);
            let unpack_sqrt_price_x64 = unpack_data.sqrt_price_x64;
            assert_eq!(unpack_sqrt_price_x64, sqrt_price_x64);
            let unpack_tick_current = unpack_data.tick_current;
            assert_eq!(unpack_tick_current, tick_current);
            let unpack_padding3 = unpack_data.padding3;
            assert_eq!(unpack_padding3, padding3);
            let unpack_padding4 = unpack_data.padding4;
            assert_eq!(unpack_padding4, padding4);
            let unpack_fee_growth_global_0_x64 = unpack_data.fee_growth_global_0_x64;
            assert_eq!(unpack_fee_growth_global_0_x64, fee_growth_global_0_x64);
            let unpack_fee_growth_global_1_x64 = unpack_data.fee_growth_global_1_x64;
            assert_eq!(unpack_fee_growth_global_1_x64, fee_growth_global_1_x64);
            let unpack_protocol_fees_token_0 = unpack_data.protocol_fees_token_0;
            assert_eq!(unpack_protocol_fees_token_0, protocol_fees_token_0);
            let unpack_protocol_fees_token_1 = unpack_data.protocol_fees_token_1;
            assert_eq!(unpack_protocol_fees_token_1, protocol_fees_token_1);
            let unpack_swap_in_amount_token_0 = unpack_data.swap_in_amount_token_0;
            assert_eq!(unpack_swap_in_amount_token_0, swap_in_amount_token_0);
            let unpack_swap_out_amount_token_1 = unpack_data.swap_out_amount_token_1;
            assert_eq!(unpack_swap_out_amount_token_1, swap_out_amount_token_1);
            let unpack_swap_in_amount_token_1 = unpack_data.swap_in_amount_token_1;
            assert_eq!(unpack_swap_in_amount_token_1, swap_in_amount_token_1);
            let unpack_swap_out_amount_token_0 = unpack_data.swap_out_amount_token_0;
            assert_eq!(unpack_swap_out_amount_token_0, swap_out_amount_token_0);
            let unpack_status = unpack_data.status;
            assert_eq!(unpack_status, status);
            let unpack_padding = unpack_data.padding;
            assert_eq!(unpack_padding, padding);

            for reward in unpack_data.reward_infos {
                let unpack_reward_state = reward.reward_state;
                assert_eq!(unpack_reward_state, reward_state);
                let unpack_open_time = reward.open_time;
                assert_eq!(unpack_open_time, open_time);
                let unpack_end_time = reward.end_time;
                assert_eq!(unpack_end_time, end_time);
                let unpack_last_update_time = reward.last_update_time;
                assert_eq!(unpack_last_update_time, last_update_time);
                let unpack_emissions_per_second_x64 = reward.emissions_per_second_x64;
                assert_eq!(unpack_emissions_per_second_x64, emissions_per_second_x64);
                let unpack_reward_total_emissioned = reward.reward_total_emissioned;
                assert_eq!(unpack_reward_total_emissioned, reward_total_emissioned);
                let unpack_reward_claimed = reward.reward_claimed;
                assert_eq!(unpack_reward_claimed, reward_claimed);
                let unpack_token_mint = reward.token_mint;
                assert_eq!(unpack_token_mint, token_mint);
                let unpack_token_vault = reward.token_vault;
                assert_eq!(unpack_token_vault, token_vault);
                let unpack_authority = reward.authority;
                assert_eq!(unpack_authority, authority);
                let unpack_reward_growth_global_x64 = reward.reward_growth_global_x64;
                assert_eq!(unpack_reward_growth_global_x64, reward_growth_global_x64);
            }

            let unpack_tick_array_bitmap = unpack_data.tick_array_bitmap;
            assert_eq!(unpack_tick_array_bitmap, tick_array_bitmap);
            let unpack_total_fees_token_0 = unpack_data.total_fees_token_0;
            assert_eq!(unpack_total_fees_token_0, total_fees_token_0);
            let unpack_total_fees_claimed_token_0 = unpack_data.total_fees_claimed_token_0;
            assert_eq!(
                unpack_total_fees_claimed_token_0,
                total_fees_claimed_token_0
            );
            let unpack_total_fees_claimed_token_1 = unpack_data.total_fees_claimed_token_1;
            assert_eq!(
                unpack_total_fees_claimed_token_1,
                total_fees_claimed_token_1
            );
            let unpack_fund_fees_token_0 = unpack_data.fund_fees_token_0;
            assert_eq!(unpack_fund_fees_token_0, fund_fees_token_0);
            let unpack_fund_fees_token_1 = unpack_data.fund_fees_token_1;
            assert_eq!(unpack_fund_fees_token_1, fund_fees_token_1);
            let unpack_open_time = unpack_data.open_time;
            assert_eq!(unpack_open_time, pool_open_time);
            let unpack_recent_epoch = unpack_data.recent_epoch;
            assert_eq!(unpack_recent_epoch, recent_epoch);
            let unpack_padding1 = unpack_data.padding1;
            assert_eq!(unpack_padding1, padding1);
            let unpack_padding2 = unpack_data.padding2;
            assert_eq!(unpack_padding2, padding2);
        }
    }
}
