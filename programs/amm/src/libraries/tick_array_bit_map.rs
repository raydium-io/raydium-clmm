///! Helper functions to get most and least significant non-zero bits
use super::big_num::U1024;
use crate::error::ErrorCode;
use crate::states::tick_array::{TickArrayState, TickState, TICK_ARRAY_SIZE};
use anchor_lang::prelude::*;

pub const TICK_ARRAY_BITMAP_SIZE: i32 = 512;

pub type TickArryBitmap = [u64; 8];

pub fn max_tick_in_tickarray_bitmap(tick_spacing: u16) -> i32 {
    i32::from(tick_spacing) * TICK_ARRAY_SIZE * TICK_ARRAY_BITMAP_SIZE
}

pub fn get_bitmap_tick_boundary(tick_array_start_index: i32, tick_spacing: u16) -> (i32, i32) {
    let ticks_in_one_bitmap: i32 = max_tick_in_tickarray_bitmap(tick_spacing);
    let mut m = tick_array_start_index.abs() / ticks_in_one_bitmap;
    if tick_array_start_index < 0 && tick_array_start_index.abs() % ticks_in_one_bitmap != 0 {
        m += 1;
    }
    let min_value: i32 = ticks_in_one_bitmap * m;
    if tick_array_start_index < 0 {
        (-min_value, -min_value + ticks_in_one_bitmap)
    } else {
        (min_value, min_value + ticks_in_one_bitmap)
    }
}

pub fn most_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(u16::try_from(x.leading_zeros()).unwrap())
    }
}

pub fn least_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(u16::try_from(x.trailing_zeros()).unwrap())
    }
}

/// Given a tick, calculate whether the tickarray it belongs to has been initialized.
/// Note: The caller of the function should ensure that tick_current is within the range represented by bit_map.
/// Currently, this function is only called when `bit_map = pool.tick_array_bitmap`.
pub fn check_current_tick_array_is_initialized(
    bit_map: U1024,
    tick_current: i32,
    tick_spacing: u16,
) -> Result<(bool, i32)> {
    if TickState::check_is_out_of_boundary(tick_current) {
        return err!(ErrorCode::InvalidTickIndex);
    }
    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    let mut compressed = tick_current / multiplier + 512;
    if tick_current < 0 && tick_current % multiplier != 0 {
        // round towards negative infinity
        compressed -= 1;
    }
    let bit_pos = compressed.abs();
    // set current bit
    let mask = U1024::one() << bit_pos.try_into().unwrap();
    let masked = bit_map & mask;
    // check the current bit whether initialized
    let initialized = masked != U1024::default();
    if initialized {
        return Ok((true, (compressed - 512) * multiplier));
    }
    // the current bit is not initialized
    return Ok((false, (compressed - 512) * multiplier));
}

/// The function is only called when `bit_map = pool.tick_array_bitmap`.
pub fn next_initialized_tick_array_start_index(
    bit_map: U1024,
    last_tick_array_start_index: i32,
    tick_spacing: u16,
    zero_for_one: bool,
) -> (bool, i32) {
    assert!(TickArrayState::check_is_valid_start_index(
        last_tick_array_start_index,
        tick_spacing
    ));
    let tick_boundary = max_tick_in_tickarray_bitmap(tick_spacing);
    let next_tick_array_start_index = if zero_for_one {
        last_tick_array_start_index - TickArrayState::tick_count(tick_spacing)
    } else {
        last_tick_array_start_index + TickArrayState::tick_count(tick_spacing)
    };

    if next_tick_array_start_index < -tick_boundary || next_tick_array_start_index >= tick_boundary
    {
        return (false, last_tick_array_start_index);
    }

    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    let mut compressed = next_tick_array_start_index / multiplier + 512;
    if next_tick_array_start_index < 0 && next_tick_array_start_index % multiplier != 0 {
        // round towards negative infinity
        compressed -= 1;
    }
    let bit_pos = compressed.abs();
    if zero_for_one {
        // tick from upper to lower
        // find from highter bits to lower bits
        let offset_bit_map = bit_map << (1024 - bit_pos - 1).try_into().unwrap();
        let next_bit = most_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            let next_array_start_index =
                (bit_pos - i32::from(next_bit.unwrap()) - 512) * multiplier;
            (true, next_array_start_index)
        } else {
            // not found til to the end
            (false, -tick_boundary)
        }
    } else {
        // tick from lower to upper
        // find from lower bits to highter bits
        let offset_bit_map = bit_map >> (bit_pos).try_into().unwrap();
        let next_bit = least_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            let next_array_start_index =
                (bit_pos + i32::from(next_bit.unwrap()) - 512) * multiplier;
            (true, next_array_start_index)
        } else {
            // not found til to the end
            (
                false,
                tick_boundary - TickArrayState::tick_count(tick_spacing),
            )
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        libraries::{tick_math, MAX_TICK},
        states::TickArrayState,
    };

    #[test]
    fn test_check_current_tick_array_is_initialized() {
        let tick_spacing = 10;
        let bit_map = U1024([
            1,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            u64::max_value() & 1 << 63,
        ]);
        let mut tick_current = -307200;
        let mut start_index = -1;
        for _i in 0..1024 {
            let ret = check_current_tick_array_is_initialized(bit_map, tick_current, tick_spacing)
                .unwrap();
            if ret.0 && ret.1 != start_index {
                start_index = ret.1;
                println!("{}-{}", tick_current, start_index);
            }
            tick_current += 600;
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_positive_price_down() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 306600;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_price_down() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -307200 + 600 + 600;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_price_down_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 1800;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }

    #[test]
    fn find_previous_init_pos_in_bit_map_positive_price_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 306600 - 600 - 600;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_price_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -307200;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_price_up_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -1800;
        for _i in 0..5 {
            let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if !is_found {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_array_start_index(array_start_index, tick_spacing);
        }
    }

    #[test]
    fn find_next_init_pos_in_bit_map_with_eigenvalues() {
        let tick_spacing = 10;
        let bit_map: [u64; 16] = [
            1,
            0,
            0,
            0,
            0,
            0,
            9223372036854775808,
            16140901064495857665,
            7,
            1,
            0,
            0,
            0,
            0,
            0,
            9223372036854775808,
        ];
        let (_, mut array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), 0, tick_spacing, true);
        assert_eq!(array_start_index, -600);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), -600, tick_spacing, true);
        assert_eq!(array_start_index, -1200);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), -1200, tick_spacing, true);
        assert_eq!(array_start_index, -1800);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), -1800, tick_spacing, true);
        assert_eq!(array_start_index, -38400);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), -38400, tick_spacing, true);
        assert_eq!(array_start_index, -39000);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), -39000, tick_spacing, true);
        assert_eq!(array_start_index, -307200);

        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), 0, tick_spacing, false);
        assert_eq!(array_start_index, 600);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), 600, tick_spacing, false);
        assert_eq!(array_start_index, 1200);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), 1200, tick_spacing, false);
        assert_eq!(array_start_index, 38400);
        (_, array_start_index) =
            next_initialized_tick_array_start_index(U1024(bit_map), 38400, tick_spacing, false);
        assert_eq!(array_start_index, 306600);
    }

    #[test]
    fn next_initialized_tick_array_start_index_boundary_test() {
        let tick_spacing = 1;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = (tick_math::MIN_TICK / TICK_ARRAY_SIZE * tick_spacing - 1)
            * TICK_ARRAY_SIZE
            * tick_spacing;
        let (is_found, array_start_index) = next_initialized_tick_array_start_index(
            bit_map,
            tick_array_start_index,
            tick_spacing as u16,
            false,
        );
        assert!(is_found == false);
        assert!(array_start_index == tick_array_start_index);

        tick_array_start_index =
            (tick_math::MAX_TICK / TICK_ARRAY_SIZE * tick_spacing) * TICK_ARRAY_SIZE * tick_spacing;
        let (is_found, array_start_index) = next_initialized_tick_array_start_index(
            bit_map,
            tick_array_start_index,
            tick_spacing as u16,
            true,
        );
        assert!(is_found == false);
        assert!(array_start_index == tick_array_start_index);
    }

    #[test]
    fn next_initialized_tick_array_with_all_initialized_bit_test() {
        let bit_map = U1024::max_value();
        for tick_spacing in [1, 10, 60] {
            let mut tick_boundary = max_tick_in_tickarray_bitmap(tick_spacing);
            if tick_boundary > MAX_TICK {
                tick_boundary = MAX_TICK;
            }
            let (min, max) = (
                TickArrayState::get_array_start_index(-tick_boundary, tick_spacing),
                TickArrayState::get_array_start_index(tick_boundary, tick_spacing),
            );
            let mut start_index = min;
            let mut expect_index;

            let loop_count = (max - start_index) / (i32::from(tick_spacing) * TICK_ARRAY_SIZE);

            for i in 0..loop_count {
                expect_index = start_index + i32::from(tick_spacing) * TICK_ARRAY_SIZE;
                let (is_found, array_start_index) = next_initialized_tick_array_start_index(
                    bit_map,
                    start_index,
                    tick_spacing as u16,
                    false,
                );

                if i < loop_count - 1 {
                    if is_found == false {
                        println!("start_index:{}", start_index)
                    }
                    assert_eq!(is_found, true);
                    assert_eq!(array_start_index, expect_index);
                    start_index = array_start_index;
                } else {
                    if tick_spacing == 60 {
                        assert_eq!(is_found, true);
                        assert_eq!(array_start_index, expect_index);
                    } else {
                        assert_eq!(is_found, false);
                        assert_eq!(array_start_index, start_index);
                        assert_eq!(
                            array_start_index,
                            max - i32::from(tick_spacing) * TICK_ARRAY_SIZE
                        )
                    }
                }
            }
        }
    }

    #[test]
    fn get_bitmap_tick_boundary_test() {
        let (mut min, mut max) = get_bitmap_tick_boundary(-430080, 1);
        assert!(min == -430080);
        assert!(max == -399360);

        (min, max) = get_bitmap_tick_boundary(-430140, 1);
        assert!(min == -460800);
        assert!(max == -430080);

        let (mut min, mut max) = get_bitmap_tick_boundary(430080, 1);
        assert!(min == 430080);
        assert!(max == 460800);

        (min, max) = get_bitmap_tick_boundary(430020, 1);
        assert!(min == 399360);
        assert!(max == 430080);
    }
}
