///! Contains functions for managing tick processes and relevant calculations
///!
use crate::libraries::{liquidity_math, tick_math};
use crate::pool::{RewardInfo, REWARD_NUM};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const TICK_SEED: &str = "tick";

/// Account storing info for a price tick
///
/// PDA of `[TICK_SEED, token_0, token_1, fee, tick]`
///
#[account]
#[derive(Default, Debug)]
pub struct TickState {
    /// Bump to identify PDA
    pub bump: u8,

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

    /// The cumulative tick value on the other side of the tick
    pub tick_cumulative_outside: i64,

    /// The seconds per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub seconds_per_liquidity_outside_x64: u128,

    /// The seconds spent on the other side of the tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub seconds_outside: u32,

    // Array of Q64.64
    pub reward_growths_outside: [u128; REWARD_NUM],
    // padding space for upgrade
    // pub padding: [u64; 8],
}

impl TickState {
    pub const LEN: usize = 8 + 1 + 4 + 16 + 16 + 16 + 16 + 8 + 16 + 4 + 16 * REWARD_NUM + 64;

    pub fn initialize(&mut self, bump: u8, tick: i32, tick_spacing: u16) -> Result<()> {
        crate::check_tick(tick, tick_spacing)?;
        self.bump = bump;
        self.tick = tick;
        Ok(())
    }
}

impl TickState {
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
    /// * `seconds_per_liquidity_cumulative_x64` - The all-time seconds per max(1, liquidity) of the pool
    /// * `tick_cumulative` - The tick * time elapsed since the pool was first initialized
    /// * `time` - The current block timestamp cast to a u32
    /// * `upper` - true for updating a position's upper tick, or false for updating a position's lower tick
    /// * `max_liquidity` - The maximum liquidity allocation for a single tick
    ///
    pub fn update(
        &mut self,
        tick_current: i32,
        liquidity_delta: i128,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        seconds_per_liquidity_cumulative_x64: u128,
        tick_cumulative: i64,
        time: u32,
        upper: bool,
        _max_liquidity: u128,
        reward_growths_outside: [u128; REWARD_NUM],
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
                self.seconds_per_liquidity_outside_x64 = seconds_per_liquidity_cumulative_x64;
                self.tick_cumulative_outside = tick_cumulative;
                self.seconds_outside = time;
                self.reward_growths_outside = reward_growths_outside;
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
    /// * `seconds_per_liquidity_cumulative_x64` - The current seconds per liquidity
    /// * `tick_cumulative` - The tick * time elapsed since the pool was first initialized
    /// * `time` - The current block timestamp
    ///
    pub fn cross(
        &mut self,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        seconds_per_liquidity_cumulative_x64: u128,
        tick_cumulative: i64,
        time: u32,
        reward_infos: &[RewardInfo; REWARD_NUM],
    ) -> i128 {
        self.fee_growth_outside_0_x64 = fee_growth_global_0_x64
            .checked_sub(self.fee_growth_outside_0_x64)
            .unwrap();
        self.fee_growth_outside_1_x64 = fee_growth_global_1_x64
            .checked_sub(self.fee_growth_outside_1_x64)
            .unwrap();
        self.seconds_per_liquidity_outside_x64 = seconds_per_liquidity_cumulative_x64
            .checked_sub(self.seconds_per_liquidity_outside_x64)
            .unwrap();
        self.tick_cumulative_outside = tick_cumulative
            .checked_sub(self.tick_cumulative_outside)
            .unwrap();
        self.seconds_outside = time.checked_sub(self.seconds_outside).unwrap();

        for i in 0..REWARD_NUM {
            if !reward_infos[i].initialized() {
                continue;
            }

            self.reward_growths_outside[i] = reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(self.reward_growths_outside[i])
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
        self.tick_cumulative_outside = 0;
        self.seconds_per_liquidity_outside_x64 = 0;
        self.seconds_outside = 0;
    }

    pub fn is_clear(self) -> bool {
        self.liquidity_net == 0
            && self.liquidity_gross == 0
            && self.fee_growth_outside_0_x64 == 0
            && self.fee_growth_outside_1_x64 == 0
            && self.tick_cumulative_outside == 0
            && self.seconds_per_liquidity_outside_x64 == 0
            && self.seconds_outside == 0
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
                .checked_sub(tick_lower.reward_growths_outside[i])
                .unwrap()
        } else {
            tick_lower.reward_growths_outside[i]
        };

        // By convention, assume all prior growth happened below the tick, not above
        let reward_growths_above = if tick_upper.liquidity_gross == 0 {
            0
        } else if tick_current_index < tick_upper.tick {
            tick_upper.reward_growths_outside[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(tick_upper.reward_growths_outside[i])
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

    mod tick_spacing_to_max_liquidity_per_tick {
        use super::*;

        #[test]
        fn lowest_fee() {
            println!("max 1 {}", tick_spacing_to_max_liquidity_per_tick(1));
            println!("max 10 {}", tick_spacing_to_max_liquidity_per_tick(10));
            println!("max 60 {}", tick_spacing_to_max_liquidity_per_tick(60));
        }

        #[test]
        fn returns_the_correct_value_for_low_fee() {
            assert_eq!(
                tick_spacing_to_max_liquidity_per_tick(10),
                415813720300916 // (2^64 - 1) / ((221810 -(-221810))/ 10 + 1)
            );
            // https://www.wolframalpha.com/input?i=%282%5E64+-+1%29+%2F+%28%28221810+-%28-221810%29%29%2F+10+%2B+1%29
        }

        #[test]
        fn returns_the_correct_value_for_medium_fee() {
            assert_eq!(
                tick_spacing_to_max_liquidity_per_tick(60),
                2495163543042006 // (2^64 - 1) / ((221760 -(-221760)) / 60 + 1)
            );
            // https://www.wolframalpha.com/input?i=%282%5E64+-+1%29+%2F+%28%28221760+-%28-221760%29%29+%2F+60+%2B+1%29
        }

        #[test]
        fn returns_the_correct_value_for_high_fee() {
            assert_eq!(
                tick_spacing_to_max_liquidity_per_tick(200),
                8313088811946620 // (2^64 - 1) / ((221800 -(-221800)) / 200 + 1)
            );
            // https://www.wolframalpha.com/input?i=%282%5E64+-+1%29+%2F+%28%28221800+-%28-221800%29%29+%2F+200+%2B+1%29
        }

        #[test]
        fn returns_the_correct_value_for_the_entire_range() {
            assert_eq!(
                tick_spacing_to_max_liquidity_per_tick(221818),
                6148914691236517205 // (2^64 - 1) / ((221818 -(-221818)) / 221818 + 1)
            );
            // https://www.wolframalpha.com/input?i=%282%5E64+-+1%29+%2F+%28%28221818+-%28-221818%29%29+%2F+221818+%2B+1%29
        }
    }

    mod get_fee_growth_inside {
        use super::*;

        #[test]
        fn returns_all_for_two_empty_ticks_if_tick_is_inside() {
            let mut tick_lower = TickState::default();
            tick_lower.tick = -2;
            let mut tick_upper = TickState::default();
            tick_upper.tick = 2;
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 0, 15, 15),
                (15, 15)
            );
        }

        #[test]
        fn returns_zero_for_two_empty_ticks_if_tick_is_above() {
            let mut tick_lower = TickState::default();
            tick_lower.tick = -2;
            let mut tick_upper = TickState::default();
            tick_upper.tick = 2;
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 4, 15, 15),
                (0, 0)
            );
        }

        #[test]
        fn returns_zero_for_two_empty_ticks_if_tick_is_below() {
            let mut tick_lower = TickState::default();
            tick_lower.tick = -2;
            let mut tick_upper = TickState::default();
            tick_upper.tick = 2;
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, -4, 15, 15),
                (0, 0)
            );
        }

        #[test]
        fn subtracts_upper_tick_if_below() {
            let mut tick_lower = TickState::default();
            tick_lower.tick = -2;
            let tick_upper = TickState {
                bump: 0,
                tick: 2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: 2,
                fee_growth_outside_1_x64: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 0, 15, 15),
                (13, 12)
            );
        }

        #[test]
        fn subtracts_lower_tick_if_above() {
            let tick_lower = TickState {
                bump: 0,
                tick: -2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: 2,
                fee_growth_outside_1_x64: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            let mut tick_upper = TickState::default();
            tick_upper.tick = 2;
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 0, 15, 15),
                (13, 12)
            );
        }

        #[test]
        fn subtracts_upper_tick_and_lower_tick_if_inside() {
            let tick_lower = TickState {
                bump: 0,
                tick: -2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: 2,
                fee_growth_outside_1_x64: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            let tick_upper = TickState {
                bump: 0,
                tick: 2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: 4,
                fee_growth_outside_1_x64: 1,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 0, 15, 15),
                (9, 11)
            );
        }

        // Run in release mode, otherwise test fails
        #[test]
        fn works_correctly_with_overflow_on_inside_tick() {
            let tick_lower = TickState {
                bump: 0,
                tick: -2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: u128::MAX - 3,
                fee_growth_outside_1_x64: u128::MAX - 2,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            let tick_upper = TickState {
                bump: 0,
                tick: 2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x64: 3,
                fee_growth_outside_1_x64: 5,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x64: 0,
                seconds_outside: 0,
                reward_growths_outside: [0; REWARD_NUM],
            };
            assert_eq!(
                get_fee_growth_inside(&tick_lower, &tick_upper, 0, 15, 15),
                (16, 13)
            );
        }
    }

    mod update {
        use super::*;

        #[test]
        fn flips_from_zero_to_non_zero() {
            let mut tick = TickState::default();
            assert!(tick
                .update(0, 1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap());
        }

        #[test]
        fn does_not_flip_from_nonzero_to_greater_nonzero() {
            let mut tick = TickState::default();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            assert!(!tick
                .update(0, 1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap());
        }

        #[test]
        fn flips_from_nonzero_to_zero() {
            let mut tick = TickState::default();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            assert!(tick
                .update(0, -1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap());
        }

        #[test]
        fn does_not_flip_from_nonzero_to_lesser_nonzero() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            assert!(!tick
                .update(0, -1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap());
        }

        #[test]
        #[should_panic(expected = "LO")]
        fn reverts_if_total_liquidity_gross_is_greater_than_max() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
        }

        #[test]
        fn nets_the_liquidity_based_on_upper_flag() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3, [0; REWARD_NUM])
                .unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, true, 10, [0; REWARD_NUM])
                .unwrap();
            tick.update(0, 3, 0, 0, 0, 0, 0, true, 10, [0; REWARD_NUM])
                .unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 10, [0; REWARD_NUM])
                .unwrap();

            assert!(tick.liquidity_gross == 2 + 1 + 3 + 1);
            assert!(tick.liquidity_net == 2 - 1 - 3 + 1);
        }

        #[test]
        #[should_panic]
        fn reverts_on_overflow_liquidity_gross() {
            let mut tick = TickState::default();
            tick.update(
                0,
                (u128::MAX / 2 - 1) as i128,
                0,
                0,
                0,
                0,
                0,
                false,
                u128::MAX,
                [0; REWARD_NUM],
            )
            .unwrap();
            tick.update(
                0,
                (u128::MAX / 2 - 1) as i128,
                0,
                0,
                0,
                0,
                0,
                false,
                u128::MAX,
                [0; REWARD_NUM],
            )
            .unwrap();
        }

        #[test]
        fn assume_all_growth_happens_below_ticks_lte_current_tick() {
            let mut tick = TickState::default();
            tick.tick = 1;
            tick.update(1, 1, 1, 2, 3, 4, 5, false, u128::MAX, [0; REWARD_NUM])
                .unwrap();

            assert!(tick.fee_growth_outside_0_x64 == 1);
            assert!(tick.fee_growth_outside_1_x64 == 2);
            assert!(tick.seconds_per_liquidity_outside_x64 == 3);
            assert!(tick.tick_cumulative_outside == 4);
            assert!(tick.seconds_outside == 5);
        }

        #[test]
        fn does_not_set_any_growth_fields_for_ticks_gt_current_tick() {
            let mut tick = TickState::default();
            tick.tick = 2;
            tick.update(1, 1, 1, 2, 3, 4, 5, false, u128::MAX, [0; REWARD_NUM])
                .unwrap();

            assert!(tick.fee_growth_outside_0_x64 == 0);
            assert!(tick.fee_growth_outside_1_x64 == 0);
            assert!(tick.seconds_per_liquidity_outside_x64 == 0);
            assert!(tick.tick_cumulative_outside == 0);
            assert!(tick.seconds_outside == 0);
        }
    }

    mod clear {
        use super::*;

        #[test]
        fn deletes_all_data_in_the_tick() {
            let mut tick = TickState {
                bump: 255,
                tick: 2,
                liquidity_net: 4,
                liquidity_gross: 3,
                fee_growth_outside_0_x64: 1,
                fee_growth_outside_1_x64: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x64: 5,
                seconds_outside: 7,
                reward_growths_outside: [0; REWARD_NUM],
            };
            tick.clear();
            assert!(tick.bump == 255);
            assert!(tick.fee_growth_outside_0_x64 == 0);
            assert!(tick.fee_growth_outside_1_x64 == 0);
            assert!(tick.seconds_per_liquidity_outside_x64 == 0);
            assert!(tick.tick_cumulative_outside == 0);
            assert!(tick.seconds_outside == 0);
            assert!(tick.liquidity_gross == 0);
            assert!(tick.liquidity_net == 0);
        }
    }

    mod cross {
        use super::*;

        #[test]
        fn flips_the_growth_variables() {
            let mut tick = TickState {
                bump: 255,
                tick: 2,
                liquidity_net: 4,
                liquidity_gross: 3,
                fee_growth_outside_0_x64: 1,
                fee_growth_outside_1_x64: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x64: 5,
                seconds_outside: 7,
                reward_growths_outside: [0; REWARD_NUM],
            };
            tick.cross(7, 9, 8, 15, 10);

            assert!(tick.fee_growth_outside_0_x64 == 6);
            assert!(tick.fee_growth_outside_1_x64 == 7);
            assert!(tick.seconds_per_liquidity_outside_x64 == 3);
            assert!(tick.tick_cumulative_outside == 9);
            assert!(tick.seconds_outside == 3);
        }

        #[test]
        fn two_flips_are_a_no_op() {
            let mut tick = TickState {
                bump: 255,
                tick: 2,
                liquidity_net: 4,
                liquidity_gross: 3,
                fee_growth_outside_0_x64: 1,
                fee_growth_outside_1_x64: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x64: 5,
                seconds_outside: 7,
                reward_growths_outside: [0; REWARD_NUM],
            };
            tick.cross(7, 9, 8, 15, 10);
            tick.cross(7, 9, 8, 15, 10);

            assert!(tick.fee_growth_outside_0_x64 == 1);
            assert!(tick.fee_growth_outside_1_x64 == 2);
            assert!(tick.seconds_per_liquidity_outside_x64 == 5);
            assert!(tick.tick_cumulative_outside == 6);
            assert!(tick.seconds_outside == 7);
        }
    }
}
