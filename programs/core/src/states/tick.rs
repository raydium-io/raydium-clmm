///! Contains functions for managing tick processes and relevant calculations
///!
use crate::error::ErrorCode;
use crate::libraries::{liquidity_math, tick_math};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const TICK_SEED: &str = "t";

/// Account storing info for a price tick
///
/// PDA of `[TICK_SEED, token_0, token_1, fee, tick]`
///
#[account(zero_copy)]
#[derive(Default, Debug)]
#[repr(packed)]
pub struct TickState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The price tick whose info is stored in the account
    pub tick: i32,

    /// The total position liquidity that references this tick
    pub liquidity_net: i64,

    /// Amount of net liquidity added (subtracted) when tick is crossed from left to right (right to left)
    pub liquidity_gross: u64,

    /// Fee growth per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub fee_growth_outside_0_x32: u64,
    pub fee_growth_outside_1_x32: u64,

    /// The cumulative tick value on the other side of the tick
    pub tick_cumulative_outside: i64,

    /// The seconds per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub seconds_per_liquidity_outside_x32: u64,

    /// The seconds spent on the other side of the tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub seconds_outside: u32,
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
    /// * `fee_growth_global_0_x32` - The all-time global fee growth, per unit of liquidity, in token_0
    /// * `fee_growth_global_1_x32` - The all-time global fee growth, per unit of liquidity, in token_1
    /// * `seconds_per_liquidity_cumulative_x32` - The all-time seconds per max(1, liquidity) of the pool
    /// * `tick_cumulative` - The tick * time elapsed since the pool was first initialized
    /// * `time` - The current block timestamp cast to a u32
    /// * `upper` - true for updating a position's upper tick, or false for updating a position's lower tick
    /// * `max_liquidity` - The maximum liquidity allocation for a single tick
    ///
    pub fn update(
        &mut self,
        tick_current: i32,
        liquidity_delta: i64,
        fee_growth_global_0_x32: u64,
        fee_growth_global_1_x32: u64,
        seconds_per_liquidity_cumulative_x32: u64,
        tick_cumulative: i64,
        time: u32,
        upper: bool,
        max_liquidity: u64,
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
                self.fee_growth_outside_0_x32 = fee_growth_global_0_x32;
                self.fee_growth_outside_1_x32 = fee_growth_global_1_x32;
                self.seconds_per_liquidity_outside_x32 = seconds_per_liquidity_cumulative_x32;
                self.tick_cumulative_outside = tick_cumulative;
                self.seconds_outside = time;
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

        Ok(flipped)
    }

    /// Transitions to the current tick as needed by price movement, returning the amount of liquidity
    /// added (subtracted) when tick is crossed from left to right (right to left)
    ///
    /// # Arguments
    ///
    /// * `self` - The destination tick of the transition
    /// * `fee_growth_global_0_x32` - The all-time global fee growth, per unit of liquidity, in token_0
    /// * `fee_growth_global_1_x32` - The all-time global fee growth, per unit of liquidity, in token_1
    /// * `seconds_per_liquidity_cumulative_x32` - The current seconds per liquidity
    /// * `tick_cumulative` - The tick * time elapsed since the pool was first initialized
    /// * `time` - The current block timestamp
    ///
    pub fn cross(
        &mut self,
        fee_growth_global_0_x32: u64,
        fee_growth_global_1_x32: u64,
        seconds_per_liquidity_cumulative_x32: u64,
        tick_cumulative: i64,
        time: u32,
    ) -> i64 {
        self.fee_growth_outside_0_x32 = fee_growth_global_0_x32 - self.fee_growth_outside_0_x32;
        self.fee_growth_outside_1_x32 = fee_growth_global_1_x32 - self.fee_growth_outside_1_x32;
        self.seconds_per_liquidity_outside_x32 =
            seconds_per_liquidity_cumulative_x32 - self.seconds_per_liquidity_outside_x32;
        self.tick_cumulative_outside = tick_cumulative - self.tick_cumulative_outside;
        self.seconds_outside = time - self.seconds_outside;

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
        self.fee_growth_outside_0_x32 = 0;
        self.fee_growth_outside_1_x32 = 0;
        self.tick_cumulative_outside = 0;
        self.seconds_per_liquidity_outside_x32 = 0;
        self.seconds_outside = 0;
    }

    pub fn is_clear(self) -> bool {
        self.liquidity_net == 0
            && self.liquidity_gross == 0
            && self.fee_growth_outside_0_x32 == 0
            && self.fee_growth_outside_1_x32 == 0
            && self.tick_cumulative_outside == 0
            && self.seconds_per_liquidity_outside_x32 == 0
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
/// * `fee_growth_global_0_x32` - The all-time global fee growth, per unit of liquidity, in token_0
/// * `fee_growth_global_1_x32` - The all-time global fee growth, per unit of liquidity, in token_1
///
pub fn get_fee_growth_inside(
    tick_lower: &TickState,
    tick_upper: &TickState,
    tick_current: i32,
    fee_growth_global_0_x32: u64,
    fee_growth_global_1_x32: u64,
) -> (u64, u64) {
    // calculate fee growth below
    let (fee_growth_below_0_x32, fee_growth_below_1_x32) = if tick_current >= tick_lower.tick {
        (
            tick_lower.fee_growth_outside_0_x32,
            tick_lower.fee_growth_outside_1_x32,
        )
    } else {
        (
            fee_growth_global_0_x32 - tick_lower.fee_growth_outside_0_x32,
            fee_growth_global_1_x32 - tick_lower.fee_growth_outside_1_x32,
        )
    };

    // Calculate fee growth above
    let (fee_growth_above_0_x32, fee_growth_above_1_x32) = if tick_current < tick_upper.tick {
        (
            tick_upper.fee_growth_outside_0_x32,
            tick_upper.fee_growth_outside_1_x32,
        )
    } else {
        (
            fee_growth_global_0_x32 - tick_upper.fee_growth_outside_0_x32,
            fee_growth_global_1_x32 - tick_upper.fee_growth_outside_1_x32,
        )
    };
    let fee_growth_inside_0_x32 =
        fee_growth_global_0_x32 - fee_growth_below_0_x32 - fee_growth_above_0_x32;
    let fee_growth_inside_1_x32 =
        fee_growth_global_1_x32 - fee_growth_below_1_x32 - fee_growth_above_1_x32;

    (fee_growth_inside_0_x32, fee_growth_inside_1_x32)
}

/// Derives max liquidity per tick from given tick spacing
///
/// # Arguments
///
/// * `tick_spacing` - The amount of required tick separation, realized in multiples of `tick_sacing`
/// e.g., a tickSpacing of 3 requires ticks to be initialized every 3rd tick i.e., ..., -6, -3, 0, 3, 6, ...
///
pub fn tick_spacing_to_max_liquidity_per_tick(tick_spacing: i32) -> u64 {
    // Find min and max values permitted by tick spacing
    let min_tick = (tick_math::MIN_TICK / tick_spacing) * tick_spacing;
    let max_tick = (tick_math::MAX_TICK / tick_spacing) * tick_spacing;
    let num_ticks = ((max_tick - min_tick) / tick_spacing) as u64 + 1;

    println!("num ticks {}", num_ticks);
    u64::MAX / num_ticks
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
                fee_growth_outside_0_x32: 2,
                fee_growth_outside_1_x32: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
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
                fee_growth_outside_0_x32: 2,
                fee_growth_outside_1_x32: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
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
                fee_growth_outside_0_x32: 2,
                fee_growth_outside_1_x32: 3,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
            };
            let tick_upper = TickState {
                bump: 0,
                tick: 2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x32: 4,
                fee_growth_outside_1_x32: 1,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
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
                fee_growth_outside_0_x32: u64::MAX - 3,
                fee_growth_outside_1_x32: u64::MAX - 2,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
            };
            let tick_upper = TickState {
                bump: 0,
                tick: 2,
                liquidity_net: 0,
                liquidity_gross: 0,
                fee_growth_outside_0_x32: 3,
                fee_growth_outside_1_x32: 5,
                tick_cumulative_outside: 0,
                seconds_per_liquidity_outside_x32: 0,
                seconds_outside: 0,
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
            assert!(tick.update(0, 1, 0, 0, 0, 0, 0, false, 3).unwrap());
        }

        #[test]
        fn does_not_flip_from_nonzero_to_greater_nonzero() {
            let mut tick = TickState::default();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3).unwrap();
            assert!(!tick.update(0, 1, 0, 0, 0, 0, 0, false, 3).unwrap());
        }

        #[test]
        fn flips_from_nonzero_to_zero() {
            let mut tick = TickState::default();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3).unwrap();
            assert!(tick.update(0, -1, 0, 0, 0, 0, 0, false, 3).unwrap());
        }

        #[test]
        fn does_not_flip_from_nonzero_to_lesser_nonzero() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3).unwrap();
            assert!(!tick.update(0, -1, 0, 0, 0, 0, 0, false, 3).unwrap());
        }

        #[test]
        #[should_panic(expected = "LO")]
        fn reverts_if_total_liquidity_gross_is_greater_than_max() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3).unwrap();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3).unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 3).unwrap();
        }

        #[test]
        fn nets_the_liquidity_based_on_upper_flag() {
            let mut tick = TickState::default();
            tick.update(0, 2, 0, 0, 0, 0, 0, false, 3).unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, true, 10).unwrap();
            tick.update(0, 3, 0, 0, 0, 0, 0, true, 10).unwrap();
            tick.update(0, 1, 0, 0, 0, 0, 0, false, 10).unwrap();

            assert!(tick.liquidity_gross == 2 + 1 + 3 + 1);
            assert!(tick.liquidity_net == 2 - 1 - 3 + 1);
        }

        #[test]
        #[should_panic]
        fn reverts_on_overflow_liquidity_gross() {
            let mut tick = TickState::default();
            tick.update(0, (u64::MAX / 2 - 1) as i64, 0, 0, 0, 0, 0, false, u64::MAX)
                .unwrap();
            tick.update(0, (u64::MAX / 2 - 1) as i64, 0, 0, 0, 0, 0, false, u64::MAX)
                .unwrap();
        }

        #[test]
        fn assume_all_growth_happens_below_ticks_lte_current_tick() {
            let mut tick = TickState::default();
            tick.tick = 1;
            tick.update(1, 1, 1, 2, 3, 4, 5, false, u64::MAX).unwrap();

            assert!(tick.fee_growth_outside_0_x32 == 1);
            assert!(tick.fee_growth_outside_1_x32 == 2);
            assert!(tick.seconds_per_liquidity_outside_x32 == 3);
            assert!(tick.tick_cumulative_outside == 4);
            assert!(tick.seconds_outside == 5);
        }

        #[test]
        fn does_not_set_any_growth_fields_for_ticks_gt_current_tick() {
            let mut tick = TickState::default();
            tick.tick = 2;
            tick.update(1, 1, 1, 2, 3, 4, 5, false, u64::MAX).unwrap();

            assert!(tick.fee_growth_outside_0_x32 == 0);
            assert!(tick.fee_growth_outside_1_x32 == 0);
            assert!(tick.seconds_per_liquidity_outside_x32 == 0);
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
                fee_growth_outside_0_x32: 1,
                fee_growth_outside_1_x32: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x32: 5,
                seconds_outside: 7,
            };
            tick.clear();
            assert!(tick.bump == 255);
            assert!(tick.fee_growth_outside_0_x32 == 0);
            assert!(tick.fee_growth_outside_1_x32 == 0);
            assert!(tick.seconds_per_liquidity_outside_x32 == 0);
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
                fee_growth_outside_0_x32: 1,
                fee_growth_outside_1_x32: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x32: 5,
                seconds_outside: 7,
            };
            tick.cross(7, 9, 8, 15, 10);

            assert!(tick.fee_growth_outside_0_x32 == 6);
            assert!(tick.fee_growth_outside_1_x32 == 7);
            assert!(tick.seconds_per_liquidity_outside_x32 == 3);
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
                fee_growth_outside_0_x32: 1,
                fee_growth_outside_1_x32: 2,
                tick_cumulative_outside: 6,
                seconds_per_liquidity_outside_x32: 5,
                seconds_outside: 7,
            };
            tick.cross(7, 9, 8, 15, 10);
            tick.cross(7, 9, 8, 15, 10);

            assert!(tick.fee_growth_outside_0_x32 == 1);
            assert!(tick.fee_growth_outside_1_x32 == 2);
            assert!(tick.seconds_per_liquidity_outside_x32 == 5);
            assert!(tick.tick_cumulative_outside == 6);
            assert!(tick.seconds_outside == 7);
        }
    }
}
