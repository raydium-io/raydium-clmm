///! Helper functions to get most and least significant non-zero bits
use super::big_num::U1024;
use crate::states::tick::TICK_ARRAY_SIZE;

/// Returns index of the most significant non-zero bit of the number
///
/// The function satisfies the property:
///     x >= 2**most_significant_bit(x) and x < 2**(most_significant_bit(x)+1)
///
/// # Arguments
///
/// * `x` - the value for which to compute the most significant bit, must be greater than 0
///
pub fn most_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(x.leading_zeros() as u16)
    }
}

/// Returns index of the least significant non-zero bit of the number
///
/// The function satisfies the property:
///     (x & 2**leastSignificantBit(x)) != 0 and (x & (2**(leastSignificantBit(x)) - 1)) == 0)
///
///
/// # Arguments
///
/// * `x` - the value for which to compute the least significant bit, must be greater than 0
///
pub fn least_significant_bit(x: U1024) -> Option<u16> {
    if x.is_zero() {
        None
    } else {
        Some(x.trailing_zeros() as u16)
    }
}

pub fn next_initialized_tick_array_start_tick(
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
        // all the 1s at or to the right of the current bit_pos
        let mask = (U1024::from(1) << bit_pos) - 1 + (U1024::from(1) << bit_pos);
        let masked = bit_map & mask;
        // check the current bit whether initialized
        let initialized = masked != U1024::default();
        // if there are no initialized ticks to the right of or at the current tick, return rightmost in the word
        let next_bit = most_significant_bit(offset_bit_map);
        if initialized && next_bit.is_some() {
            Some((compressed + 1 + next_bit.unwrap() as i32 - 512) * multiplier)
        } else {
            // the current bit is not initialized or find to the end
            None
        }
    } else {
        // tick from lower to upper
        // find from lower bits to highter bits
        let offset_bit_map = bit_map >> (1024 - bit_pos);
        // all the 1s at or to the left of the bitPos
        let mask = !((U1024::from(1) << bit_pos) - 1);
        let masked = bit_map & mask;
        // if there are no initialized ticks to the left of the current tick, return leftmost in the word
        let initialized = masked != U1024::default();
        let next_bit = least_significant_bit(offset_bit_map);
        if initialized && next_bit.is_some() {
            Some((compressed - 1 - next_bit.unwrap() as i32 - 512) * multiplier)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::states::TickArrayState;

    #[test]
    fn find_next_init_pos_in_bit_map_positive_up() {
        let tick_spacing = 10;
        let bit_map = U1024::max_value();
        let mut tick_array_start_index = 0;
        for _i in 0..5 {
            let array_start_index = next_initialized_tick_array_start_tick(
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
            let array_start_index = next_initialized_tick_array_start_tick(
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
            let array_start_index = next_initialized_tick_array_start_tick(
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
            let array_start_index = next_initialized_tick_array_start_tick(
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
            let array_start_index = next_initialized_tick_array_start_tick(
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
            let array_start_index = next_initialized_tick_array_start_tick(
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
