use super::big_num::U1024;
use crate::error::ErrorCode;
use crate::states::tick_array::{TickArrayState, TickState, TICK_ARRAY_SIZE};
use anchor_lang::prelude::*;

pub const TICK_ARRAY_BITMAP_SIZE: i32 = 512;
pub const TOTAL_BITMAP_SIZE: usize = 1024;

pub type TickArryBitmap = [u64; 8];

/// Returns the maximum tick representable in the tick array bitmap
pub fn max_tick_in_tickarray_bitmap(tick_spacing: u16) -> i32 {
    i32::from(tick_spacing) * TICK_ARRAY_SIZE * TICK_ARRAY_BITMAP_SIZE
}

/// Gets the min and max tick boundaries for a bitmap containing the given tick array
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

/// Helper function to calculate bit position (0-1023) for a given tick array start index
fn calculate_bit_pos(tick_array_start_index: i32, tick_spacing: u16) -> Result<usize> {
    // Ensure input is a valid start index (implicitly checks divisibility)
    // Note: If calling code already validated, this might be redundant, but safer.
    if !TickArrayState::check_is_valid_start_index(tick_array_start_index, tick_spacing) {
         msg!("Input tick {} is not a valid start index for tick spacing {}",
             tick_array_start_index, tick_spacing);
        return err!(ErrorCode::InvalidTickIndex);
    }

    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    // Direct calculation: division result + offset gives 0-1023 index
    let compressed = tick_array_start_index / multiplier + 512;

    // Ensure calculated bit position is within valid range [0, 1023]
    if !(0..TOTAL_BITMAP_SIZE as i32).contains(&compressed) {
        msg!("Calculated bit position {} out of range [0, {}] for tick {}",
             compressed, TOTAL_BITMAP_SIZE - 1, tick_array_start_index);
        // This case should ideally not happen if tick boundaries are enforced elsewhere,
        // but provides safety.
        return err!(ErrorCode::InvalidTickIndex);
    }

    Ok(compressed as usize)
}

/// Finds the tick array start index corresponding to a given bit position
fn find_tick_for_bit_pos(bit_pos: usize, tick_spacing: u16) -> i32 {
    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    (bit_pos as i32 - 512) * multiplier
}

/// Given a tick, calculate whether the tickarray it belongs to has been initialized.
/// Returns (is_initialized, tick_array_start_index)
pub fn check_current_tick_array_is_initialized(
    bit_map: U1024,
    tick_current: i32,
    tick_spacing: u16,
) -> Result<(bool, i32)> {
    if TickState::check_is_out_of_boundary(tick_current) {
        return err!(ErrorCode::InvalidTickIndex);
    }
    
    // Find the start index of the array containing this tick
    let tick_array_start_index = TickArrayState::get_array_start_index(tick_current, tick_spacing);
    
    // Calculate bit position for this tick array
    let bit_pos = match calculate_bit_pos(tick_array_start_index, tick_spacing) {
        Ok(pos) => pos,
        Err(e) => return Err(e),
    };
    
    // Check if the bit is set in the bitmap
    let mask = U1024::one() << bit_pos;
    let initialized = (bit_map & mask) != U1024::default();
    
    Ok((initialized, tick_array_start_index))
}

/// Checks if a bit is set in the bitmap at the specified position
fn is_bit_set(bitmap: &U1024, bit_pos: usize) -> bool {
    if bit_pos >= TOTAL_BITMAP_SIZE {
        return false;
    }
    
    let word_idx = bit_pos / 64;
    let bit_in_word = bit_pos % 64;
    (bitmap.0[word_idx] & (1u64 << bit_in_word)) != 0
}

/// Creates a mask covering all bits below the specified position
fn create_lower_mask(bit_pos: usize) -> U1024 {
    // Early return for edge cases
    if bit_pos == 0 {
        return U1024::default(); // Empty mask
    }
    if bit_pos >= TOTAL_BITMAP_SIZE {
        return U1024::max_value(); // All bits set
    }
    
    let mut mask = U1024::default();
    
    // Set all complete words
    let word_idx = bit_pos / 64;
    for i in 0..word_idx {
        mask.0[i] = u64::MAX;
    }
    
    // Set bits in the partial word
    let bit_in_word = bit_pos % 64;
    if bit_in_word > 0 {
        mask.0[word_idx] = (1u64 << bit_in_word) - 1;
    }
    
    mask
}

/// Creates a mask covering all bits above the specified position
fn create_upper_mask(bit_pos: usize) -> U1024 {
    // Early return for edge cases
    if bit_pos >= TOTAL_BITMAP_SIZE - 1 {
        return U1024::default(); // Empty mask
    }
    
    let mut mask = U1024::default();
    
    // Set all complete words above
    let word_idx = bit_pos / 64;
    for i in (word_idx + 1)..16 {
        mask.0[i] = u64::MAX;
    }
    
    // Set bits in the partial word
    let bit_in_word = bit_pos % 64;
    if bit_in_word < 63 {
        mask.0[word_idx] = !((1u64 << (bit_in_word + 1)) - 1);
    }
    
    mask
}

/// Validates tick array start index compatibility with tick spacing
///
/// For high tick spacings (≥15), only checks if the index is a multiple of the 
/// tick array size * tick spacing. For lower tick spacings, uses the standard validation.
fn validate_tick_array_start_index(tick_array_start_index: i32, tick_spacing: u16) -> bool {
    let multiplier = i32::from(tick_spacing) * TICK_ARRAY_SIZE;
    
    // Basic divisibility check that works for all tick spacings
    if tick_array_start_index % multiplier != 0 {
        return false;
    }
    
    // For small tick spacings, use the standard validation logic which includes
    // additional checks beyond just divisibility
    if tick_spacing < 15 {
        return TickArrayState::check_is_valid_start_index(tick_array_start_index, tick_spacing);
    }
    
    // For high tick spacings, divisibility is the only requirement
    true
}

/// Finds the next initialized tick array start index in the bitmap
/// 
/// # Parameters
/// - `bit_map`: The bitmap representing initialized tick arrays
/// - `last_tick_array_start_index`: The current tick array start index
/// - `tick_spacing`: The tick spacing of the pool
/// - `zero_for_one`: Direction (true = searching downward, false = searching upward)
/// 
/// # Returns
/// - (found, next_start_index): where found is true if an initialized array was found
pub fn next_initialized_tick_array_start_index(
    bit_map: U1024,
    last_tick_array_start_index: i32,
    tick_spacing: u16,
    zero_for_one: bool,
) -> (bool, i32) {
    // Validate the start index
    if !validate_tick_array_start_index(last_tick_array_start_index, tick_spacing) {
        msg!("Invalid tick array start index: {} for tick spacing: {}", 
             last_tick_array_start_index, tick_spacing);
        return (false, last_tick_array_start_index);
    }
    
    // Calculate boundaries
    let tick_boundary = max_tick_in_tickarray_bitmap(tick_spacing);
    let max_valid_tick = tick_boundary - TickArrayState::tick_count(tick_spacing);
    
    // Check if we're beyond the representable range
    if last_tick_array_start_index < -tick_boundary || last_tick_array_start_index >= tick_boundary {
        return (false, last_tick_array_start_index);
    }

    // Get bit position for the current tick array
    let current_bit_pos = match calculate_bit_pos(last_tick_array_start_index, tick_spacing) {
        Ok(pos) => pos,
        Err(_) => {
            // Fallback to boundary if calculation fails
            return (false, if zero_for_one { -tick_boundary } else { max_valid_tick });
        }
    };
    
    // Handle search based on direction
    if zero_for_one {
        // DOWNWARD SEARCH (from high to low bits)
        
        // First, handle special cases
        
        // Case 1: At bit 1023 (max valid tick) check if it's set
        if current_bit_pos == TOTAL_BITMAP_SIZE - 1 && is_bit_set(&bit_map, TOTAL_BITMAP_SIZE - 1) {
            return (true, last_tick_array_start_index);
        }
        
        // Case 2: At bit 0 (min valid tick) we can't go lower
        if current_bit_pos == 0 {
            return (false, -tick_boundary);
        }
        
        // Standard search - apply mask and find MSB
        let mask = create_lower_mask(current_bit_pos);
        let search_area = bit_map & mask;
        
        if search_area.is_zero() {
            return (false, -tick_boundary);
        }
        
        // Find highest set bit in the masked area
        let msb_idx = most_significant_bit(search_area)
            .map(|lz| TOTAL_BITMAP_SIZE - 1 - usize::from(lz))
            .unwrap_or(0);
            
        (true, find_tick_for_bit_pos(msb_idx, tick_spacing))
    } else {
        // UPWARD SEARCH (from low to high bits)
        
        // Case: At or beyond max bit position
        if current_bit_pos >= TOTAL_BITMAP_SIZE - 1 {
            return (false, max_valid_tick);
        }
        
        // Standard search - apply mask and find LSB
        let mask = create_upper_mask(current_bit_pos);
        let search_area = bit_map & mask;
        
        if search_area.is_zero() {
            return (false, max_valid_tick);
        }
        
        // Find lowest set bit in the masked area
        let lsb_idx = least_significant_bit(search_area)
            .map(usize::from)
            .unwrap_or(0);
            
        (true, find_tick_for_bit_pos(lsb_idx, tick_spacing))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{libraries::tick_math, states::TickArrayState};

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
