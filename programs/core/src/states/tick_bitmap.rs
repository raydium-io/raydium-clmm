use crate::error::ErrorCode;
///! Packed tick initialized state library
///! Stores a packed mapping of tick index to its initialized state
///
///! Although ticks are stored as i32, all tick values fit within 24 bits.
///! Therefore the mapping uses i16 for keys and there are 256 (2^8) values per word.
///!
use crate::libraries::big_num::U256;
use crate::libraries::bit_math;
use crate::libraries::tick_math;
use crate::util::*;
use anchor_lang::{prelude::*, system_program};
use std::ops::BitXor;

/// Seed to derive account address and signature
pub const BITMAP_SEED: &str = "tick_bitmap";

/// Stores info for a single bitmap word.
/// Each word represents 256 packed tick initialized boolean values.
///
/// Emulates a solidity mapping, where word_position is the key and
///
/// PDA of `[BITMAP_SEED, token_0, token_1, fee, word_pos]`
///
#[account(zero_copy)]
#[derive(Default, Debug)]
#[repr(packed)]
pub struct TickBitmapState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The bitmap key. To find word position from a tick, divide the tick by tick spacing
    /// to get a 24 bit compressed result, then right shift to obtain the most significant 16 bits.
    pub word_pos: i16,

    /// Packed initialized state
    pub word: [u64; 4],
}

impl TickBitmapState {
    pub const LEN: usize = 8 + 1 + 2 + 8 * 4;

    pub fn get_or_create_tick_bitmap<'info>(
        payer: AccountInfo<'info>,
        bitmap_account_info: AccountInfo<'info>,
        system_program: AccountInfo<'info>,
        pool_key: Pubkey,
        word_pos: i16,
        tick_spacing: u16,
    ) -> Result<AccountLoader<'info, TickBitmapState>> {
        let mut is_create = false;
        let mut pda_bump: u8 = 0;
        let bitmap_state = if bitmap_account_info.owner == &system_program::ID {
            let (expect_pda_address, bump) = Pubkey::find_program_address(
                &[
                    BITMAP_SEED.as_bytes(),
                    pool_key.as_ref(),
                    &word_pos.to_be_bytes(),
                ],
                &crate::id(),
            );
            require_keys_eq!(expect_pda_address, bitmap_account_info.key());
            create_or_allocate_account(
                &crate::id(),
                payer,
                system_program,
                bitmap_account_info.clone(),
                &[
                    BITMAP_SEED.as_bytes(),
                    pool_key.as_ref(),
                    &word_pos.to_be_bytes(),
                    &[bump],
                ],
                TickBitmapState::LEN,
            )?;
            is_create = true;
            pda_bump = bump;
            AccountLoader::<TickBitmapState>::try_from_unchecked(
                &crate::id(),
                &bitmap_account_info,
            )?
        } else {
            AccountLoader::<TickBitmapState>::try_from(&bitmap_account_info)?
        };

        if is_create {
            {
                let mut bitmap_account = bitmap_state.load_init()?;
                bitmap_account.initialize(pda_bump, word_pos, tick_spacing)?;
            }
            bitmap_state.exit(&crate::id())?;
        }
        Ok(bitmap_state)
    }

    pub fn initialize(&mut self, bump: u8, word_pos: i16, tick_spacing: u16) -> Result<()> {
        let max_word_pos = ((tick_math::MAX_TICK / tick_spacing as i32) >> 8) as i16;
        let min_word_pos = ((tick_math::MIN_TICK / tick_spacing as i32) >> 8) as i16;
        require!(word_pos >= min_word_pos, ErrorCode::TickLowerOverflow);
        require!(word_pos <= max_word_pos, ErrorCode::TickUpperOverflow);

        self.bump = bump;
        self.word_pos = word_pos;
        Ok(())
    }
}

/// The position in the mapping where the initialized bit for a tick lives
pub struct Position {
    /// The key in the mapping containing the word in which the bit is stored
    pub word_pos: i16,

    /// The bit position in the word where the flag is stored
    pub bit_pos: u8,
}

/// The next initialized bit
#[derive(Debug)]
pub struct NextBit {
    /// The relative position of the next initialized or uninitialized tick up to 256 ticks away from the current tick
    pub next: u8,

    /// Whether the next tick is initialized, as the function only searches within up to 256 ticks
    pub initialized: bool,
}

/// Computes the bitmap position for a bit.
///
/// # Arguments
///
/// * `tick_by_spacing` - The tick for which to compute the position, divided by tick spacing
///
pub fn position(tick_by_spacing: i32) -> Position {
    Position {
        word_pos: (tick_by_spacing >> 8) as i16,
        // begins with 255 for negative numbers
        bit_pos: (tick_by_spacing % 256) as u8,
    }
}

impl TickBitmapState {
    ///  Flips the initialized state for a given bit from false to true, or vice versa
    ///
    /// # Arguments
    ///
    /// * `self` - The bitmap state corresponding to the tick's word position
    /// * `bit_pos` - The rightmost 8 bits of the tick
    ///
    pub fn flip_bit(&mut self, bit_pos: u8) {
        let word = U256(self.word);
        let mask = U256::from(1) << bit_pos;
        self.word = word.bitxor(mask).0;
    }

    /// Returns the bitmap index (0 - 255) for the next initialized tick.
    ///
    /// If no initialized tick is available, returns the first bit (index 0) the word in lte case,
    /// and the last bit in gte case.
    ///
    /// Unlike Uniswap, this checks for equality in lte = false case. Externally ensure that
    /// `compressed + 1` is used to derive the word_pos(for bitmap account) and bit_pos.
    ///
    /// # Obtain the actual tick using
    ///
    /// ```rs
    /// (next + 255 * word_pos) * spacing
    /// ```
    ///
    /// # Arguments
    ///
    /// * `self` - The mapping in which to compute the next initialized tick
    /// * `bit_pos` - The starting bit position
    /// * `lte` - Whether to search for the next initialized tick to the left (less than or equal to the starting tick)
    ///
    pub fn next_initialized_bit(&self, bit_pos: u8, lte: bool) -> NextBit {
        let word = U256(self.word);
        if lte {
            // all the 1s at or to the right of the current bit_pos
            let mask = (U256::from(1) << bit_pos) - 1 + (U256::from(1) << bit_pos);
            let masked = word & mask;
            let initialized = masked != U256::default();

            // if there are no initialized ticks to the right of or at the current tick, return rightmost in the word
            let next = if initialized {
                bit_math::most_significant_bit(masked)
            } else {
                0
            };

            NextBit { next, initialized }
        } else {
            // all the 1s at or to the left of the bit_pos
            let mask = !((U256::from(1) << bit_pos) - 1);
            let masked = word & mask;
            let initialized = masked != U256::default();

            // if there are no initialized ticks to the left of the current tick, return leftmost in the word
            let next = if initialized {
                bit_math::least_significant_bit(masked)
            } else {
                u8::MAX
            };

            NextBit { next, initialized }
        }
    }

    /// Whether the tick at given bit position is initialized
    #[cfg(test)]
    fn is_initialized(self, bit_pos: u8) -> bool {
        let next_bit = self.next_initialized_bit(bit_pos, true);
        next_bit.next == bit_pos && next_bit.initialized
    }

    /// Initialize bits at the given positions
    #[cfg(test)]
    fn init_bits(&mut self, bit_positions: &[u8]) {
        for bit_pos in bit_positions {
            self.flip_bit(*bit_pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod is_initialized {
        use super::*;

        #[test]
        fn is_false_at_first() {
            let tick_bitmap = TickBitmapState::default();
            assert!(!tick_bitmap.is_initialized(1));
        }

        #[test]
        fn is_flipped_by_flip_tick() {
            let mut tick_bitmap = TickBitmapState::default();
            tick_bitmap.flip_bit(1);
            assert!(tick_bitmap.is_initialized(1));
        }

        #[test]
        fn is_flipped_back_by_flip_tick() {
            let mut tick_bitmap = TickBitmapState::default();
            tick_bitmap.flip_bit(1);
            tick_bitmap.flip_bit(1);
            assert!(!tick_bitmap.is_initialized(1));
        }

        #[test]
        fn is_not_changed_by_another_flip_to_a_different_tick() {
            let mut tick_bitmap = TickBitmapState::default();
            tick_bitmap.flip_bit(2);
            assert!(!tick_bitmap.is_initialized(1));
        }
    }

    mod is_flipped {
        use super::*;

        #[test]
        fn flips_only_the_specified_tick() {
            let mut tick_bitmap = TickBitmapState::default();
            tick_bitmap.flip_bit(230);
            assert!(tick_bitmap.is_initialized(230));
            assert!(!tick_bitmap.is_initialized(229));
            assert!(!tick_bitmap.is_initialized(231));

            tick_bitmap.flip_bit(230);
            assert!(!tick_bitmap.is_initialized(230));
            assert!(!tick_bitmap.is_initialized(229));
            assert!(!tick_bitmap.is_initialized(231));
        }
    }

    mod next_initialized_bit_within_one_word {
        use super::*;

        mod lte_is_false {
            use super::*;

            #[test]
            fn returns_same_bit_if_initialized() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(78, false);
                assert!(initialized);
                assert_eq!(next, 78);
            }

            #[test]
            fn returns_bit_at_right_if_at_uninitialized_bit() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);

                // to simulate greater than condition, use bit_pos + 1
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(78 + 1, false);
                assert!(initialized);
                assert_eq!(next, 84);
            }

            #[test]
            fn does_not_exceed_boundary_if_no_initialized_bit() {
                let tick_bitmap = TickBitmapState::default();
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(0, false);
                assert!(!initialized);
                assert_eq!(next, 255);
            }
        }

        mod lte_is_true {
            use super::*;

            #[test]
            fn returns_same_bit_if_initialized() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(78, true);
                assert!(initialized);
                assert_eq!(next, 78);
            }

            #[test]
            fn returns_bit_directly_to_the_left_of_input_bit_if_not_initialized() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(79, true);
                assert!(initialized);
                assert_eq!(next, 78);
            }

            #[test]
            fn will_not_exceed_lower_boundary() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(1, true);
                assert!(!initialized);
                assert_eq!(next, 0);
            }

            #[test]
            fn at_the_lower_boundary() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(0, true);
                assert!(!initialized);
                assert_eq!(next, 0);
            }

            #[test]
            fn returns_bit_at_left_if_not_initialized() {
                let mut tick_bitmap = TickBitmapState::default();
                tick_bitmap.init_bits(&[70, 78, 84, 139, 240]);
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(71, true);
                assert!(initialized);
                assert_eq!(next, 70);
            }

            #[test]
            fn entire_empty_word() {
                let tick_bitmap = TickBitmapState::default();
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(255, true);
                assert!(!initialized);
                assert_eq!(next, 0);
            }

            #[test]
            fn halfway_through_empty_word() {
                let tick_bitmap = TickBitmapState::default();
                let NextBit { next, initialized } = tick_bitmap.next_initialized_bit(127, true);
                assert!(!initialized);
                assert_eq!(next, 0);
            }
        }
    }
}
