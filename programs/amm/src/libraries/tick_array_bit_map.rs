///! Helper functions to get most and least significant non-zero bits
use super::big_num::U1024;
use crate::states::tick::TICK_ARRAY_SIZE;

pub fn most_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(x.leading_zeros() as u16)
    }
}

pub fn least_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(x.trailing_zeros() as u16)
    }
}

pub fn check_current_tick_array_is_initialized(
    bit_map: U1024,
    tick_current: i32,
    tick_spacing: i32,
) -> (bool, Option<i32>) {
    let multiplier = tick_spacing as i32 * TICK_ARRAY_SIZE;
    let mut compressed = tick_current / multiplier + 512;
    if tick_current < 0 && tick_current % multiplier != 0 {
        // round towards negative infinity
        compressed -= 1;
    }
    let bit_pos = compressed.abs();
    // set current bit
    let mask = U1024::from(1) << bit_pos;
    let masked = bit_map & mask;
    // check the current bit whether initialized
    let initialized = masked != U1024::default();
    if initialized {
        (true, Some(bit_pos))
    } else {
        // the current bit is not initialized
        (false, None)
    }
}

pub fn next_initialized_tick_array_start_index(
    bit_map: U1024,
    tick_array_start_index: i32,
    tick_spacing: i32,
    zero_for_one: bool,
) -> Option<i32> {
    let multiplier = tick_spacing as i32 * TICK_ARRAY_SIZE;
    let mut compressed = tick_array_start_index / multiplier + 512;
    if tick_array_start_index < 0 && tick_array_start_index % multiplier != 0 {
        // round towards negative infinity
        compressed -= 1;
    }
    let bit_pos = compressed.abs();

    if zero_for_one {
        // tick from upper to lower
        // find from highter bits to lower bits
        let offset_bit_map = bit_map << (bit_pos + 1);
        let next_bit = most_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            Some((compressed + 1 + next_bit.unwrap() as i32 - 512) * multiplier)
        } else {
            // not found til to the end
            None
        }
    } else {
        // tick from lower to upper
        // find from lower bits to highter bits
        let offset_bit_map = bit_map >> (1024 - bit_pos);
        let next_bit = least_significant_bit(offset_bit_map);
        if next_bit.is_some() {
            Some((compressed - 1 - next_bit.unwrap() as i32 - 512) * multiplier)
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
        let mut tick_current = -409600;
        let mut bit_pos_index = -1;
        for _i in 0..1024 {
            let ret = check_current_tick_array_is_initialized(bit_map, tick_current, tick_spacing);
            if ret.0 && ret.1 != Some(bit_pos_index) {
                bit_pos_index = ret.1.unwrap();
                println!("{}-{}", tick_current, bit_pos_index);
            }
            tick_current += 800;
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_positive_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 0;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -409600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_next_init_pos_in_bit_map_negative_up_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -1600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                true,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }

    #[test]
    fn find_previous_init_pos_in_bit_map_positive_down() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 408800;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_down() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = -800;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
    #[test]
    fn find_previous_init_pos_in_bit_map_negative_down_crose_zero() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 1600;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_index(
                bit_map,
                tick_array_start_index,
                tick_spacing,
                false,
            );
            println!("{:?}", array_start_index);
            tick_array_start_index =
                TickArrayState::get_arrary_start_index(array_start_index.unwrap(), tick_spacing);
        }
    }
}
