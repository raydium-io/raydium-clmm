///! Helper functions to get most and least significant non-zero bits
use super::big_num::U1024;
use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::states::tick_array::{
    MAX_TICK_ARRAY_START_INDEX, MIN_TICK_ARRAY_START_INDEX, TICK_ARRAY_SIZE,
};
use anchor_lang::{require, Result};

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

pub fn check_current_tick_array_is_initialized(
    bit_map: U1024,
    tick_current: i32,
    tick_spacing: i32,
) -> Result<(bool, i32)> {
    require!(
        tick_current >= tick_math::MIN_TICK && tick_current <= tick_math::MAX_TICK,
        ErrorCode::InvaildTickIndex
    );
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

pub fn next_initialized_tick_array_start_index(
    bit_map: U1024,
    tick_array_start_index: i32,
    tick_spacing: i32,
    zero_for_one: bool,
) -> Option<i32> {
    assert!(
        tick_array_start_index >= MIN_TICK_ARRAY_START_INDEX
            && tick_array_start_index <= MAX_TICK_ARRAY_START_INDEX
    );
    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    let mut compressed = tick_array_start_index / multiplier + 512;
    if tick_array_start_index < 0 && tick_array_start_index % multiplier != 0 {
        // round towards negative infinity
        compressed -= 1;
    }
    let bit_pos = compressed.abs();

    if zero_for_one {
        // tick from upper to lower
        // find from highter bits to lower bits
        let offset_bit_map = bit_map << (1024 - bit_pos).try_into().unwrap();
        let next_bit = most_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            let next_array_start_index =
                (bit_pos - 1 - i32::from(next_bit.unwrap()) - 512) * multiplier;
            Some(next_array_start_index)
        } else {
            // not found til to the end
            None
        }
    } else {
        // tick from lower to upper
        // find from lower bits to highter bits
        let offset_bit_map = bit_map >> (bit_pos + 1).try_into().unwrap();
        let next_bit = least_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            let next_array_start_index =
                (bit_pos + 1 + i32::from(next_bit.unwrap()) - 512) * multiplier;
            Some(next_array_start_index)
        } else {
            // not found til to the end
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::states::TickArrayState;

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
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_price_down() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -307200 + 600 + 600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_price_down_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 1600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }

    #[test]
    fn find_previous_init_pos_in_bit_map_positive_price_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 306600 - 600 - 600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_price_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -307200;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_price_up_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -1600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            if array_start_index.is_none() {
                break;
            }
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
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
        let mut array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), 0, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -600);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), -600, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -1200);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), -1200, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -1800);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), -1800, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -38400);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), -38400, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -39000);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), -39000, tick_spacing, true);
        assert_eq!(array_start_index.unwrap(), -307200);

        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), 0, tick_spacing, false);
        assert_eq!(array_start_index.unwrap(), 600);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), 600, tick_spacing, false);
        assert_eq!(array_start_index.unwrap(), 1200);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), 1200, tick_spacing, false);
        assert_eq!(array_start_index.unwrap(), 38400);
        array_start_index =
            next_initialized_tick_array_start_index(U1024(bit_map), 38400, tick_spacing, false);
        assert_eq!(array_start_index.unwrap(), 306600);
    }

    #[test]
    fn next_initialized_tick_array_start_index_boundary_test() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = MAX_TICK_ARRAY_START_INDEX;
        let array_start_index = next_initialized_tick_array_start_index(
            bit_map,
            tick_array_start_index,
            tick_spacing,
            false,
        );
        assert!(array_start_index.is_none());

        tick_array_start_index = MIN_TICK_ARRAY_START_INDEX;
        let array_start_index = next_initialized_tick_array_start_index(
            bit_map,
            tick_array_start_index,
            tick_spacing,
            true,
        );
        assert!(array_start_index.is_none());
    }
}
