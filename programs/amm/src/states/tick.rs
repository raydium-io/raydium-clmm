use crate::error::ErrorCode;
///! Contains functions for managing tick processes and relevant calculations
///!
use crate::libraries::{liquidity_math, tick_math};
use crate::pool::{RewardInfo, REWARD_NUM};
use crate::util::*;
use anchor_lang::{prelude::*, system_program};

/// Seed to derive account address and signature
pub const TICK_ARRAY_SEED: &str = "tick_array";
pub const TICK_ARRAY_SIZE_USIZE: usize = 80;
pub const TICK_ARRAY_SIZE: i32 = 80;

#[account(zero_copy)]
#[repr(packed)]
pub struct TickArrayState {
    pub amm_pool: Pubkey,
    pub start_tick_index: i32,
    pub ticks: [TickState; TICK_ARRAY_SIZE_USIZE],
}

impl TickArrayState {
    pub const LEN: usize = 8 + 32 + 4 + TickState::LEN * TICK_ARRAY_SIZE_USIZE;

    pub fn get_or_create_tick_array<'info>(
        payer: AccountInfo<'info>,
        tick_array_account_info: AccountInfo<'info>,
        system_program: AccountInfo<'info>,
        pool_key: Pubkey,
        start_index: i32,
        tick_spacing: u16,
    ) -> Result<AccountLoader<'info, TickArrayState>> {
        let mut is_create = false;
        let tick_array_state = if tick_array_account_info.owner == &system_program::ID {
            let (expect_pda_address, bump) = Pubkey::find_program_address(
                &[
                    TICK_ARRAY_SEED.as_bytes(),
                    pool_key.as_ref(),
                    &start_index.to_be_bytes(),
                ],
                &crate::id(),
            );
            require_keys_eq!(expect_pda_address, tick_array_account_info.key());
            create_or_allocate_account(
                &crate::id(),
                payer,
                system_program,
                tick_array_account_info.clone(),
                &[
                    TICK_ARRAY_SEED.as_bytes(),
                    pool_key.as_ref(),
                    &start_index.to_be_bytes(),
                    &[bump],
                ],
                TickArrayState::LEN,
            )?;
            is_create = true;
            AccountLoader::<TickArrayState>::try_from_unchecked(
                &crate::id(),
                &tick_array_account_info,
            )?
        } else {
            AccountLoader::<TickArrayState>::try_from(&tick_array_account_info)?
        };

        if is_create {
            {
                let mut tick_array_account = tick_array_state.load_init()?;
                tick_array_account.initialize(start_index, tick_spacing, pool_key)?;
            }
            tick_array_state.exit(&crate::id())?;
        }
        Ok(tick_array_state)
    }

    pub fn initialize(
        &mut self,
        start_index: i32,
        tick_spacing: u16,
        pool_key: Pubkey,
    ) -> Result<()> {
        require_eq!(0, start_index % (TICK_ARRAY_SIZE * (tick_spacing) as i32));
        self.start_tick_index = start_index;
        self.amm_pool = pool_key;
        Ok(())
    }

    pub fn first_initialized_tick(&mut self, zero_for_one: bool) -> Result<&mut TickState> {
        if zero_for_one {
            let mut i = TICK_ARRAY_SIZE - 1;
            while i >= 0 {
                if self.ticks[i as usize].is_initialized() {
                    return Ok(self.ticks.get_mut(i as usize).unwrap());
                }
                i = i - 1;
            }
        } else {
            let mut i = 0;
            while i < TICK_ARRAY_SIZE_USIZE {
                if self.ticks[i].is_initialized() {
                    return Ok(self.ticks.get_mut(i).unwrap());
                }
                i = i + 1;
            }
        }
        err!(ErrorCode::InvalidTickArray)
    }

    pub fn next_initialized_tick(
        &mut self,
        currenct_tick_index: i32,
        tick_spacing: u16,
        zero_for_one: bool,
    ) -> Result<Option<&mut TickState>> {
        let start_tick_index =
            TickArrayState::get_arrary_start_index(currenct_tick_index, tick_spacing as i32);
        let is_start_index = start_tick_index == currenct_tick_index;
        let mut index_in_array =
            (currenct_tick_index - self.start_tick_index) / (tick_spacing as i32);
        if zero_for_one {
            if is_start_index {
                index_in_array = index_in_array - 1;
            }
            while index_in_array >= 0 {
                if self.ticks[index_in_array as usize].is_initialized() {
                    return Ok(self.ticks.get_mut(index_in_array as usize));
                }
                index_in_array = index_in_array - 1;
            }
        } else {
            if is_start_index {
                index_in_array = index_in_array + 1;
            }
            while index_in_array < TICK_ARRAY_SIZE {
                if self.ticks[index_in_array as usize].is_initialized() {
                    return Ok(self.ticks.get_mut(index_in_array as usize));
                }
                index_in_array = index_in_array + 1;
            }
        }
        Ok(None)
    }

    pub fn get_tick_state_mut(
        &mut self,
        tick_index: i32,
        tick_spacing: i32,
    ) -> Result<&mut TickState> {
        require_eq!(0, tick_index % tick_spacing);
        let start_tick_index = TickArrayState::get_arrary_start_index(tick_index, tick_spacing);
        require_eq!(
            start_tick_index,
            self.start_tick_index,
            ErrorCode::InvalidTickArray
        );
        let index_in_array = ((tick_index - self.start_tick_index) / tick_spacing) as usize;
        Ok(&mut self.ticks[index_in_array])
    }

    pub fn next_tick_arrary_start_index(&self, tick_spacing: u16, zero_for_one: bool) -> i32 {
        if zero_for_one {
            self.start_tick_index - (tick_spacing as i32) * TICK_ARRAY_SIZE
        } else {
            self.start_tick_index + (tick_spacing as i32) * TICK_ARRAY_SIZE
        }
    }

    pub fn get_arrary_start_index(tick_index: i32, tick_spacing: i32) -> i32 {
        let mut start = tick_index / (tick_spacing * TICK_ARRAY_SIZE);
        if tick_index < 0 {
            start = start - 1
        }
        start * (tick_spacing * TICK_ARRAY_SIZE)
    }
}

impl Default for TickArrayState {
    #[inline]
    fn default() -> TickArrayState {
        TickArrayState {
            amm_pool: Pubkey::default(),
            ticks: [TickState::default(); TICK_ARRAY_SIZE_USIZE],
            start_tick_index: 0,
        }
    }
}

/// Account storing info for a price tick
///
/// PDA of `[TICK_SEED, token_0, token_1, fee, tick]`
///
#[zero_copy]
#[repr(packed)]
#[derive(Default, Debug)]
pub struct TickState {
    /// The price tick whose info is stored in the account
    pub tick: i32,

    /// Amount of net liquidity added (subtracted) when tick is crossed from left to right (right to left)
    pub liquidity_net: i128,
    /// The total position liquidity that references this tick
    pub liquidity_gross: u128,

    /// Fee growth per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub fee_growth_outside_0_x64: u128,
    pub fee_growth_outside_1_x64: u128,

    // Array of Q64.64
    pub reward_growths_outside_x64: [u128; REWARD_NUM],
    // padding space for upgrade
    // pub padding: [u64; 8],
}

impl TickState {
    pub const LEN: usize = 4 + 16 + 16 + 16 + 16 + 16 * REWARD_NUM;

    pub fn initialize(&mut self, tick: i32, tick_spacing: u16) -> Result<()> {
        check_tick_boundary(tick, tick_spacing)?;
        self.tick = tick;
        Ok(())
    }

    /// Updates a tick and returns true if the tick was flipped from initialized to uninitialized, or vice versa
    ///
    /// # Arguments
    ///
    /// * `self` - The tick state that will be updated
    /// * `tick_current` - The current tick
    /// * `liquidity_delta` - A new amount of liquidity to be added (subtracted) when tick is crossed
    /// from left to right (right to left)
    /// * `fee_growth_global_0_x64` - The all-time global fee growth, per unit of liquidity, in token_0
    /// * `fee_growth_global_1_x64` - The all-time global fee growth, per unit of liquidity, in token_1
    /// * `upper` - true for updating a position's upper tick, or false for updating a position's lower tick
    /// * `max_liquidity` - The maximum liquidity allocation for a single tick
    ///
    pub fn update(
        &mut self,
        tick_current: i32,
        liquidity_delta: i128,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        upper: bool,
        _max_liquidity: u128,
        reward_growths_outside_x64: [u128; REWARD_NUM],
    ) -> Result<bool> {
        let liquidity_gross_before = self.liquidity_gross;
        let liquidity_gross_after =
            liquidity_math::add_delta(liquidity_gross_before, liquidity_delta)?;

        // Overflow should not happen for sane pools
        // entire tick range can never be traversed
        // require!(liquidity_gross_after <= max_liquidity, ErrorCode::LO);

        // Either liquidity_gross_after becomes 0 (uninitialized) XOR liquidity_gross_before
        // was zero (initialized)
        let flipped = (liquidity_gross_after == 0) != (liquidity_gross_before == 0);

        if liquidity_gross_before == 0 {
            // by convention, we assume that all growth before a tick was initialized happened _below_ the tick
            if self.tick <= tick_current {
                self.fee_growth_outside_0_x64 = fee_growth_global_0_x64;
                self.fee_growth_outside_1_x64 = fee_growth_global_1_x64;
                self.reward_growths_outside_x64 = reward_growths_outside_x64;
            }
        }

        self.liquidity_gross = liquidity_gross_after;

        // when the lower (upper) tick is crossed left to right (right to left),
        // liquidity must be added (removed)
        self.liquidity_net = if upper {
            self.liquidity_net.checked_sub(liquidity_delta)
        } else {
            self.liquidity_net.checked_add(liquidity_delta)
        }
        .unwrap();
        // msg!("tick update,liquidity_gross_before:{}, liquidity_gross_after:{},liquidity_net:{}, upper:{}", liquidity_gross_before, liquidity_gross_after, self.liquidity_net,upper);
        Ok(flipped)
    }

    /// Transitions to the current tick as needed by price movement, returning the amount of liquidity
    /// added (subtracted) when tick is crossed from left to right (right to left)
    ///
    /// # Arguments
    ///
    /// * `self` - The destination tick of the transition
    /// * `fee_growth_global_0_x64` - The all-time global fee growth, per unit of liquidity, in token_0
    /// * `fee_growth_global_1_x64` - The all-time global fee growth, per unit of liquidity, in token_1
    ///
    pub fn cross(
        &mut self,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        reward_infos: &[RewardInfo; REWARD_NUM],
    ) -> i128 {
        self.fee_growth_outside_0_x64 = fee_growth_global_0_x64
            .checked_sub(self.fee_growth_outside_0_x64)
            .unwrap();
        self.fee_growth_outside_1_x64 = fee_growth_global_1_x64
            .checked_sub(self.fee_growth_outside_1_x64)
            .unwrap();

        for i in 0..REWARD_NUM {
            if !reward_infos[i].initialized() {
                continue;
            }

            self.reward_growths_outside_x64[i] = reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(self.reward_growths_outside_x64[i])
                .unwrap();
        }

        self.liquidity_net
    }

    /// Clears tick data. Variables other than bump and tick are cleared
    ///
    /// # Arguments
    ///
    /// * `self` - The tick account to be cleared
    ///
    pub fn clear(&mut self) {
        self.liquidity_net = 0;
        self.liquidity_gross = 0;
        self.fee_growth_outside_0_x64 = 0;
        self.fee_growth_outside_1_x64 = 0;
        self.reward_growths_outside_x64 = [0; REWARD_NUM];
    }

    pub fn is_initialized(self) -> bool {
        self.liquidity_gross != 0
    }

    pub fn is_clear(self) -> bool {
        self.liquidity_net == 0
            && self.liquidity_gross == 0
            && self.fee_growth_outside_0_x64 == 0
            && self.fee_growth_outside_1_x64 == 0
    }
}

/// Retrieves the all time fee growth data in token_0 and token_1, per unit of liquidity,
/// inside a position's tick boundaries.
///
/// Calculates `fr = fg - f_below(lower) - f_above(upper)`, formula 6.19
///
/// # Arguments
///
/// * `tick_lower` - The lower tick boundary of the position
/// * `tick_upper` - The upper tick boundary of the position
/// * `tick_current` - The current tick
/// * `fee_growth_global_0_x64` - The all-time global fee growth, per unit of liquidity, in token_0
/// * `fee_growth_global_1_x64` - The all-time global fee growth, per unit of liquidity, in token_1
///
pub fn get_fee_growth_inside(
    tick_lower: &TickState,
    tick_upper: &TickState,
    tick_current: i32,
    fee_growth_global_0_x64: u128,
    fee_growth_global_1_x64: u128,
) -> (u128, u128) {
    // calculate fee growth below
    let (fee_growth_below_0_x64, fee_growth_below_1_x64) = if tick_current >= tick_lower.tick {
        (
            tick_lower.fee_growth_outside_0_x64,
            tick_lower.fee_growth_outside_1_x64,
        )
    } else {
        (
            fee_growth_global_0_x64
                .checked_sub(tick_lower.fee_growth_outside_0_x64)
                .unwrap(),
            fee_growth_global_1_x64
                .checked_sub(tick_lower.fee_growth_outside_1_x64)
                .unwrap(),
        )
    };

    // Calculate fee growth above
    let (fee_growth_above_0_x64, fee_growth_above_1_x64) = if tick_current < tick_upper.tick {
        (
            tick_upper.fee_growth_outside_0_x64,
            tick_upper.fee_growth_outside_1_x64,
        )
    } else {
        (
            fee_growth_global_0_x64
                .checked_sub(tick_upper.fee_growth_outside_0_x64)
                .unwrap(),
            fee_growth_global_1_x64
                .checked_sub(tick_upper.fee_growth_outside_1_x64)
                .unwrap(),
        )
    };
    let fee_growth_inside_0_x64 = fee_growth_global_0_x64
        .checked_sub(fee_growth_below_0_x64)
        .unwrap()
        .checked_sub(fee_growth_above_0_x64)
        .unwrap();
    let fee_growth_inside_1_x64 = fee_growth_global_1_x64
        .checked_sub(fee_growth_below_1_x64)
        .unwrap()
        .checked_sub(fee_growth_above_1_x64)
        .unwrap();

    (fee_growth_inside_0_x64, fee_growth_inside_1_x64)
}

// Calculates the reward growths inside of tick_lower and tick_upper based on their positions
// relative to tick_current. An uninitialized reward will always have a reward growth of zero.
pub fn get_reward_growths_inside(
    tick_lower: &TickState,
    tick_upper: &TickState,
    tick_current_index: i32,
    reward_infos: &[RewardInfo; REWARD_NUM],
) -> ([u128; REWARD_NUM]) {
    let mut reward_growths_inside = [0; REWARD_NUM];

    for i in 0..REWARD_NUM {
        if !reward_infos[i].initialized() {
            continue;
        }

        // By convention, assume all prior growth happened below the tick
        let reward_growths_below = if tick_lower.liquidity_gross == 0 {
            reward_infos[i].reward_growth_global_x64
        } else if tick_current_index < tick_lower.tick {
            reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(tick_lower.reward_growths_outside_x64[i])
                .unwrap()
        } else {
            tick_lower.reward_growths_outside_x64[i]
        };

        // By convention, assume all prior growth happened below the tick, not above
        let reward_growths_above = if tick_upper.liquidity_gross == 0 {
            0
        } else if tick_current_index < tick_upper.tick {
            tick_upper.reward_growths_outside_x64[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(tick_upper.reward_growths_outside_x64[i])
                .unwrap()
        };

        reward_growths_inside[i] = reward_infos[i]
            .reward_growth_global_x64
            .checked_sub(reward_growths_below)
            .unwrap()
            .checked_sub(reward_growths_above)
            .unwrap();
    }

    reward_growths_inside
}

/// Common checks for a valid tick input.
/// A tick is valid iff it lies within tick boundaries and it is a multiple
/// of tick spacing.
///
/// # Arguments
///
/// * `tick` - The price tick
///
pub fn check_tick_boundary(tick: i32, tick_spacing: u16) -> Result<()> {
    require!(tick >= tick_math::MIN_TICK, ErrorCode::TickLowerOverflow);
    require!(tick <= tick_math::MAX_TICK, ErrorCode::TickUpperOverflow);
    require!(
        tick % tick_spacing as i32 == 0,
        ErrorCode::TickAndSpacingNotMatch
    );
    Ok(())
}

/// Common checks for valid tick inputs.
///
/// # Arguments
///
/// * `tick_lower` - The lower tick
/// * `tick_upper` - The upper tick
///
pub fn check_ticks_order(tick_lower_index: i32, tick_upper_index: i32) -> Result<()> {
    require!(
        tick_lower_index < tick_upper_index,
        ErrorCode::TickInvaildOrder
    );
    Ok(())
}

/// Derives max liquidity per tick from given tick spacing
///
/// # Arguments
///
/// * `tick_spacing` - The amount of required tick separation, realized in multiples of `tick_sacing`
/// e.g., a tickSpacing of 3 requires ticks to be initialized every 3rd tick i.e., ..., -6, -3, 0, 3, 6, ...
///
pub fn tick_spacing_to_max_liquidity_per_tick(tick_spacing: i32) -> u128 {
    // Find min and max values permitted by tick spacing
    let min_tick = (tick_math::MIN_TICK / tick_spacing) * tick_spacing;
    let max_tick = (tick_math::MAX_TICK / tick_spacing) * tick_spacing;
    let num_ticks = ((max_tick - min_tick) / tick_spacing) as u64 + 1;

    println!("num ticks {}", num_ticks);
    u128::MAX / (num_ticks as u128)
}

#[cfg(test)]
mod test {
    use super::*;
    mod tick_array_test {

        use super::*;

        #[test]
        fn get_arrary_start_index_test() {
            assert_eq!(TickArrayState::get_arrary_start_index(120, 3), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(1002, 30), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(-120, 3), -240);
            assert_eq!(TickArrayState::get_arrary_start_index(-1002, 30), -2400);
            assert_eq!(TickArrayState::get_arrary_start_index(-20, 10), -800);
            assert_eq!(TickArrayState::get_arrary_start_index(20, 10), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(-1002, 10), -1600);
        }

        #[test]
        fn next_tick_arrary_start_index_test() {
            let tick_array = &mut TickArrayState::default();
            tick_array.initialize(-2400, 15, Pubkey::default()).unwrap();
            // println!("{:?}", tick_array);
            assert_eq!(-3600, tick_array.next_tick_arrary_start_index(15, true));
            assert_eq!(-1200, tick_array.next_tick_arrary_start_index(15, false));
        }

        #[test]
        fn first_initialized_tick_test() {
            let tick_array = &mut TickArrayState::default();
            tick_array.initialize(-1200, 15, Pubkey::default()).unwrap();
            let mut tick_state = tick_array.get_tick_state_mut(-300, 15).unwrap();
            tick_state.liquidity_gross = 1;
            tick_state.tick = -300;
            tick_state = tick_array.get_tick_state_mut(-15, 15).unwrap();
            tick_state.liquidity_gross = 1;
            tick_state.tick = -15;

            {
                let tick = tick_array.first_initialized_tick(false).unwrap().tick;
                assert_eq!(-300, tick);
            }
            {
                let tick = tick_array.first_initialized_tick(true).unwrap().tick;
                assert_eq!(-15, tick);
            }
        }
    }
}
