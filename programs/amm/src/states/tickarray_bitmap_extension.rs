use crate::error::ErrorCode;
use crate::libraries::{
    big_num::U512,
    tick_array_bit_map::{
        get_bitmap_tick_boundary, max_tick_in_tickarray_bitmap, TickArryBitmap,
        TICK_ARRAY_BITMAP_SIZE,
    },
    tick_math,
};
use crate::states::{TickArrayState, POOL_TICK_ARRAY_BITMAP_SEED};
use anchor_lang::prelude::*;
use std::ops::BitXor;

const EXTENSION_TICKARRAY_BITMAP_SIZE: usize = 14;

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Debug)]
pub struct TickArrayBitmapExtension {
    pub pool_id: Pubkey,
    /// Packed initialized tick array state for start_tick_index is positive
    pub positive_tick_array_bitmap: [[u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
    /// Packed initialized tick array state for start_tick_index is negitive
    pub negative_tick_array_bitmap: [[u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
}

impl Default for TickArrayBitmapExtension {
    #[inline]
    fn default() -> TickArrayBitmapExtension {
        TickArrayBitmapExtension {
            pool_id: Pubkey::default(),
            positive_tick_array_bitmap: [[0; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
            negative_tick_array_bitmap: [[0; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
        }
    }
}

impl TickArrayBitmapExtension {
    pub const LEN: usize = 8 + 32 + 64 * EXTENSION_TICKARRAY_BITMAP_SIZE * 2;

    pub fn initialize(&mut self, pool_id: Pubkey) {
        self.pool_id = pool_id;
        self.positive_tick_array_bitmap = [[0; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];
        self.negative_tick_array_bitmap = [[0; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];
    }

    pub fn key(pool_id: Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(), pool_id.as_ref()],
            &crate::id(),
        )
        .0
    }

    fn get_bitmap_offset(tick_index: i32, tick_spacing: u16) -> Result<usize> {
        require!(
            TickArrayState::check_is_valid_start_index(tick_index, tick_spacing),
            ErrorCode::InvaildTickIndex
        );
        Self::check_extension_boundary(tick_index, tick_spacing)?;
        let ticks_in_one_bitmap = max_tick_in_tickarray_bitmap(tick_spacing);
        let mut offset = tick_index.abs() / ticks_in_one_bitmap - 1;
        if tick_index < 0 && tick_index.abs() % ticks_in_one_bitmap == 0 {
            offset -= 1;
        }
        Ok(offset as usize)
    }

    /// According to the given tick, calculate its corresponding tickarray and then find the bitmap it belongs to.
    fn get_bitmap(&self, tick_index: i32, tick_spacing: u16) -> Result<(usize, TickArryBitmap)> {
        let offset = Self::get_bitmap_offset(tick_index, tick_spacing)?;
        if tick_index < 0 {
            Ok((offset, self.negative_tick_array_bitmap[offset]))
        } else {
            Ok((offset, self.positive_tick_array_bitmap[offset]))
        }
    }

    /// Check if the tick in tick array bitmap extension
    pub fn check_extension_boundary(tick_index: i32, tick_spacing: u16) -> Result<()> {
        let positive_tick_boundary = max_tick_in_tickarray_bitmap(tick_spacing);
        let negative_tick_boundary = -positive_tick_boundary;
        require_gt!(tick_math::MAX_TICK, positive_tick_boundary);
        require_gt!(negative_tick_boundary, tick_math::MIN_TICK);
        if tick_index >= negative_tick_boundary && tick_index < positive_tick_boundary {
            return err!(ErrorCode::InvalidTickArrayBoundary);
        }
        Ok(())
    }

    /// Check if the tick array is initialized
    pub fn check_tick_array_is_initialized(
        &self,
        tick_array_start_index: i32,
        tick_spacing: u16,
    ) -> Result<(bool, i32)> {
        let (_, tickarray_bitmap) = self.get_bitmap(tick_array_start_index, tick_spacing)?;

        let tick_array_offset_in_bitmap =
            Self::tick_array_offset_in_bitmap(tick_array_start_index, tick_spacing);

        if U512(tickarray_bitmap).bit(tick_array_offset_in_bitmap as usize) {
            return Ok((true, tick_array_start_index));
        }
        Ok((false, tick_array_start_index))
    }

    /// Flip the value of tick in the bitmap.
    pub fn flip_tick_array_bit(
        &mut self,
        tick_array_start_index: i32,
        tick_spacing: u16,
    ) -> Result<()> {
        let (offset, tick_array_bitmap) = self.get_bitmap(tick_array_start_index, tick_spacing)?;
        let tick_array_offset_in_bitmap =
            Self::tick_array_offset_in_bitmap(tick_array_start_index, tick_spacing);
        let tick_array_bitmap = U512(tick_array_bitmap);
        let mask = U512::one() << tick_array_offset_in_bitmap;
        if tick_array_start_index < 0 {
            self.negative_tick_array_bitmap[offset as usize] = tick_array_bitmap.bitxor(mask).0;
        } else {
            self.positive_tick_array_bitmap[offset as usize] = tick_array_bitmap.bitxor(mask).0;
        }
        Ok(())
    }

    /// Search for the first initialized bit in bitmap according to the direction, if found return ture and the tick array start index,
    /// if not, return false and tick boundary index
    pub fn next_initialized_tick_array_from_one_bitmap(
        &self,
        last_tick_array_start_index: i32,
        tick_spacing: u16,
        zero_for_one: bool,
    ) -> Result<(bool, i32)> {
        let multiplier = TickArrayState::tick_count(tick_spacing);
        let next_tick_array_start_index = if zero_for_one {
            last_tick_array_start_index - multiplier
        } else {
            last_tick_array_start_index + multiplier
        };
        let min_tick_array_start_index =
            TickArrayState::get_array_start_index(tick_math::MIN_TICK, tick_spacing);
        let max_tick_array_start_index =
            TickArrayState::get_array_start_index(tick_math::MAX_TICK, tick_spacing);

        if next_tick_array_start_index < min_tick_array_start_index
            || next_tick_array_start_index > max_tick_array_start_index
        {
            return Ok((false, next_tick_array_start_index));
        }

        let (_, tickarray_bitmap) = self.get_bitmap(next_tick_array_start_index, tick_spacing)?;

        Ok(Self::next_initialized_tick_array_in_bitmap(
            tickarray_bitmap,
            next_tick_array_start_index,
            tick_spacing,
            zero_for_one,
        ))
    }

    pub fn next_initialized_tick_array_in_bitmap(
        tickarray_bitmap: TickArryBitmap,
        next_tick_array_start_index: i32,
        tick_spacing: u16,
        zero_for_one: bool,
    ) -> (bool, i32) {
        let (bitmap_min_tick_boundary, bitmap_max_tick_boundary) =
            get_bitmap_tick_boundary(next_tick_array_start_index, tick_spacing);

        let tick_array_offset_in_bitmap =
            Self::tick_array_offset_in_bitmap(next_tick_array_start_index, tick_spacing);
        if zero_for_one {
            // tick from upper to lower
            // find from highter bits to lower bits
            let offset_bit_map = U512(tickarray_bitmap)
                << (TICK_ARRAY_BITMAP_SIZE - 1 - tick_array_offset_in_bitmap);

            let next_bit = if offset_bit_map.is_zero() {
                None
            } else {
                Some(u16::try_from(offset_bit_map.leading_zeros()).unwrap())
            };

            if next_bit.is_some() {
                let next_array_start_index = next_tick_array_start_index
                    - i32::from(next_bit.unwrap()) * TickArrayState::tick_count(tick_spacing);
                return (true, next_array_start_index);
            } else {
                // not found til to the end
                return (false, bitmap_min_tick_boundary);
            }
        } else {
            // tick from lower to upper
            // find from lower bits to highter bits
            let offset_bit_map = U512(tickarray_bitmap) >> tick_array_offset_in_bitmap;

            let next_bit = if offset_bit_map.is_zero() {
                None
            } else {
                Some(u16::try_from(offset_bit_map.trailing_zeros()).unwrap())
            };
            if next_bit.is_some() {
                let next_array_start_index = next_tick_array_start_index
                    + i32::from(next_bit.unwrap()) * TickArrayState::tick_count(tick_spacing);
                return (true, next_array_start_index);
            } else {
                // not found til to the end
                return (
                    false,
                    bitmap_max_tick_boundary - TickArrayState::tick_count(tick_spacing),
                );
            }
        }
    }

    pub fn tick_array_offset_in_bitmap(tick_array_start_index: i32, tick_spacing: u16) -> i32 {
        let m = tick_array_start_index.abs() % max_tick_in_tickarray_bitmap(tick_spacing);
        let mut tick_array_offset_in_bitmap = m / TickArrayState::tick_count(tick_spacing);
        if tick_array_start_index < 0 && m != 0 {
            tick_array_offset_in_bitmap = TICK_ARRAY_BITMAP_SIZE - tick_array_offset_in_bitmap;
        }
        tick_array_offset_in_bitmap
    }
}

#[cfg(test)]
pub mod tick_array_bitmap_extension_test {
    use std::str::FromStr;

    use super::*;
    use crate::tick_array::TICK_ARRAY_SIZE;

    pub fn flip_tick_array_bit_helper(
        tick_array_bitmap_extension: &mut TickArrayBitmapExtension,
        tick_spacing: u16,
        init_tick_array_start_indexs: Vec<i32>,
    ) {
        for start_index in init_tick_array_start_indexs {
            tick_array_bitmap_extension
                .flip_tick_array_bit(start_index, tick_spacing)
                .unwrap();
        }
    }

    pub struct BuildExtensionAccountInfo {
        pub key: Pubkey,
        pub lamports: u64,
        pub owner: Pubkey,
        pub data: Vec<u8>,
    }

    impl Default for BuildExtensionAccountInfo {
        #[inline]
        fn default() -> BuildExtensionAccountInfo {
            BuildExtensionAccountInfo {
                key: Pubkey::new_unique(),
                lamports: 0,
                owner: Pubkey::from_str("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK").unwrap(),
                data: vec![0; 1832],
            }
        }
    }

    pub fn build_tick_array_bitmap_extension_info<'info>(
        param: &mut BuildExtensionAccountInfo,
    ) -> AccountInfo {
        let disc_bytes = [60, 150, 36, 219, 97, 128, 139, 153];
        for i in 0..8 {
            param.data[i] = disc_bytes[i];
        }
        AccountInfo::new(
            &param.key,
            false,
            true,
            &mut param.lamports,
            param.data.as_mut_slice(),
            &param.owner,
            false,
            0,
        )
    }

    #[test]
    fn get_bitmap_offset_test() {
        let tick_spacing = 1;
        let tick_spacing_u16: u16 = 1;
        let mut offset = TickArrayBitmapExtension::get_bitmap_offset(
            tick_spacing * TICK_ARRAY_SIZE * 512,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 0);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            tick_spacing * TICK_ARRAY_SIZE * 513,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 0);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            tick_spacing * TICK_ARRAY_SIZE * 1024,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 1);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            tick_spacing * TICK_ARRAY_SIZE * 7393,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 13);

        let mut offset = TickArrayBitmapExtension::get_bitmap_offset(
            -tick_spacing * TICK_ARRAY_SIZE * 513,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 0);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            -tick_spacing * TICK_ARRAY_SIZE * 1024,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 0);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            -tick_spacing * TICK_ARRAY_SIZE * 1025,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 1);

        offset = TickArrayBitmapExtension::get_bitmap_offset(
            -tick_spacing * TICK_ARRAY_SIZE * 7394,
            tick_spacing_u16,
        )
        .unwrap();
        assert!(offset == 13);
    }

    #[test]
    fn get_bitmap_test() {
        let tick_spacing = 1;
        let tick_array_bitmap_extension = TickArrayBitmapExtension::default();

        let offset = tick_array_bitmap_extension
            .get_bitmap(tick_spacing * TICK_ARRAY_SIZE * 511, tick_spacing as u16)
            .is_err();
        assert!(offset == true);

        let (offset, _) = tick_array_bitmap_extension
            .get_bitmap(tick_spacing * TICK_ARRAY_SIZE * 512, tick_spacing as u16)
            .unwrap();
        assert!(offset == 0);

        let (offset, _) = tick_array_bitmap_extension
            .get_bitmap(tick_spacing * TICK_ARRAY_SIZE * 1024, tick_spacing as u16)
            .unwrap();
        assert!(offset == 1);

        let offset = tick_array_bitmap_extension
            .get_bitmap(-tick_spacing * TICK_ARRAY_SIZE * 512, tick_spacing as u16)
            .is_err();
        assert!(offset == true);

        let (offset, _) = tick_array_bitmap_extension
            .get_bitmap(-tick_spacing * TICK_ARRAY_SIZE * 513, tick_spacing as u16)
            .unwrap();
        assert!(offset == 0);
    }

    #[test]
    fn flip_tick_array_bit_test() {
        let tick_array_bitmap_extension = &mut TickArrayBitmapExtension::default();
        let tick_spacing = 1;
        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                tick_spacing * TICK_ARRAY_SIZE * 512, // min positvie tick array start index boundary in extension
                tick_spacing * TICK_ARRAY_SIZE * 513,
                tick_spacing * TICK_ARRAY_SIZE * 7393, // max positvie tick array start index boundary in extension
                -tick_spacing * TICK_ARRAY_SIZE * 513, // min negative tick array start index boundary in extension
                -tick_spacing * TICK_ARRAY_SIZE * 514,
                -tick_spacing * TICK_ARRAY_SIZE * 1024,
                -tick_spacing * TICK_ARRAY_SIZE * 7394, // max negative tick array start index boundary in extension
            ],
        );

        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(0) == true);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(1) == true);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[13]).bit(225) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(511) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(510) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(0) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[13]).bit(286) == true);

        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                tick_spacing * TICK_ARRAY_SIZE * 512, // min positvie tick array start index boundary in extension
                tick_spacing * TICK_ARRAY_SIZE * 513,
                tick_spacing * TICK_ARRAY_SIZE * 7393, // max positvie tick array start index boundary in extension
                -tick_spacing * TICK_ARRAY_SIZE * 513, // min negative tick array start index boundary in extension
                -tick_spacing * TICK_ARRAY_SIZE * 514,
                -tick_spacing * TICK_ARRAY_SIZE * 7394, // max negative tick array start index boundary in extension
            ],
        );
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(0) == false);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(1) == false);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[13]).bit(225) == false);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(511) == false);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(510) == false);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[13]).bit(286) == false);

        let tick_array_bitmap_extension = &mut TickArrayBitmapExtension::default();
        let tick_spacing = 3;
        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                tick_spacing * TICK_ARRAY_SIZE * 512,
                tick_spacing * TICK_ARRAY_SIZE * 2464,
                -tick_spacing * TICK_ARRAY_SIZE * 513,
                -tick_spacing * TICK_ARRAY_SIZE * 2465,
            ],
        );

        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(0) == true);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[3]).bit(416) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(511) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[3]).bit(95) == true);

        let tick_array_bitmap_extension = &mut TickArrayBitmapExtension::default();
        let tick_spacing = 10;
        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                tick_spacing * TICK_ARRAY_SIZE * 512,
                tick_spacing * TICK_ARRAY_SIZE * 739,
                -tick_spacing * TICK_ARRAY_SIZE * 513,
                -tick_spacing * TICK_ARRAY_SIZE * 740,
            ],
        );

        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(0) == true);
        assert!(U512(tick_array_bitmap_extension.positive_tick_array_bitmap[0]).bit(227) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(511) == true);
        assert!(U512(tick_array_bitmap_extension.negative_tick_array_bitmap[0]).bit(284) == true);
    }

    #[test]
    fn positive_next_initialized_tick_array_start_index_test() {
        let tick_spacing = 1;
        let tick_array_bitmap_extension = &mut TickArrayBitmapExtension::default();
        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                tick_spacing * TICK_ARRAY_SIZE * 512, // min positvie tick array start index boundary in extension
                tick_spacing * TICK_ARRAY_SIZE * 1000,
                tick_spacing * TICK_ARRAY_SIZE * 7393, // max positvie tick array start index boundary in extension
            ],
        );

        // one_for_zero, look for in the direction of a larger tick.
        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                tick_spacing * TICK_ARRAY_SIZE * 511,
                tick_spacing as u16,
                false,
            )
            .unwrap();
        assert!(next == tick_spacing * TICK_ARRAY_SIZE * 512);

        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                tick_spacing * TICK_ARRAY_SIZE * 512,
                tick_spacing as u16,
                false,
            )
            .unwrap();
        assert!(next == tick_spacing * TICK_ARRAY_SIZE * 1000);

        let next = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                tick_spacing * TICK_ARRAY_SIZE * 7393,
                tick_spacing as u16,
                false,
            )
            .unwrap();
        assert!(next.0 == false);

        // zero_for_one.
        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                tick_spacing * TICK_ARRAY_SIZE * 1001,
                tick_spacing as u16,
                true,
            )
            .unwrap();
        assert!(next == tick_spacing * TICK_ARRAY_SIZE * 1000);

        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                tick_spacing * TICK_ARRAY_SIZE * 1000,
                tick_spacing as u16,
                true,
            )
            .unwrap();
        assert!(next == tick_spacing * TICK_ARRAY_SIZE * 512);

        // zero_for_one, last tickarray start index is too little, not reach the extension boundary value.
        let next = tick_array_bitmap_extension.next_initialized_tick_array_from_one_bitmap(
            tick_spacing * TICK_ARRAY_SIZE * 512,
            tick_spacing as u16,
            true,
        );
        assert!(next.is_err());
    }

    #[test]
    fn negative_next_initialized_tick_array_start_index_test() {
        let tick_spacing = 1;
        let tick_array_bitmap_extension = &mut TickArrayBitmapExtension::default();
        flip_tick_array_bit_helper(
            tick_array_bitmap_extension,
            tick_spacing as u16,
            vec![
                -tick_spacing * TICK_ARRAY_SIZE * 513, // min negative tick array start index boundary in extension
                -tick_spacing * TICK_ARRAY_SIZE * 1000,
                -tick_spacing * TICK_ARRAY_SIZE * 7394, // max negative tick array start index boundary in extension
            ],
        );

        // one_for_zero, look for in the direction of a larger tick.
        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                -tick_spacing * TICK_ARRAY_SIZE * 1001,
                tick_spacing as u16,
                false,
            )
            .unwrap();
        assert!(next == -tick_spacing * TICK_ARRAY_SIZE * 1000);

        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                -tick_spacing * TICK_ARRAY_SIZE * 1000,
                tick_spacing as u16,
                false,
            )
            .unwrap();
        assert!(next == -tick_spacing * TICK_ARRAY_SIZE * 513);

        let next = tick_array_bitmap_extension.next_initialized_tick_array_from_one_bitmap(
            -tick_spacing * TICK_ARRAY_SIZE * 513,
            tick_spacing as u16,
            false,
        );
        assert!(next.is_err());

        // zero_for_one.
        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                -tick_spacing * TICK_ARRAY_SIZE * 512,
                tick_spacing as u16,
                true,
            )
            .unwrap();
        assert!(next == -tick_spacing * TICK_ARRAY_SIZE * 513);

        let (_, next) = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                -tick_spacing * TICK_ARRAY_SIZE * 513,
                tick_spacing as u16,
                true,
            )
            .unwrap();
        assert!(next == -tick_spacing * TICK_ARRAY_SIZE * 1000);

        // zero_for_one, last tickarray start index is too little, not reach the extension boundary value.
        let next = tick_array_bitmap_extension
            .next_initialized_tick_array_from_one_bitmap(
                -tick_spacing * TICK_ARRAY_SIZE * 7394,
                tick_spacing as u16,
                true,
            )
            .unwrap();
        assert!(next.0 == false);
    }

    #[test]
    fn bitmap_extension_layout_test() {
        use anchor_lang::Discriminator;

        let pool_id = Pubkey::new_unique();
        let mut positive_tick_array_bitmap = [[0u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];
        let mut negative_tick_array_bitmap = [[0u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];

        // serialize original data
        let mut bitmap_extension_data = [0u8; 8 + 32 + 64 * EXTENSION_TICKARRAY_BITMAP_SIZE * 2];
        let mut offset = 0;
        bitmap_extension_data[offset..offset + 8]
            .copy_from_slice(&TickArrayBitmapExtension::discriminator());
        offset += 8;
        bitmap_extension_data[offset..offset + 32].copy_from_slice(&pool_id.to_bytes());
        offset += 32;

        let mut init_data = u64::MAX;
        for i in 0..EXTENSION_TICKARRAY_BITMAP_SIZE {
            for j in 0..8 {
                init_data -= 1;
                positive_tick_array_bitmap[i][j] = init_data;
                bitmap_extension_data[offset..offset + 8].copy_from_slice(&init_data.to_le_bytes());
                offset += 8;
            }
        }
        for i in 0..EXTENSION_TICKARRAY_BITMAP_SIZE {
            for j in 0..8 {
                init_data -= 1;
                negative_tick_array_bitmap[i][j] = init_data;
                bitmap_extension_data[offset..offset + 8].copy_from_slice(&init_data.to_le_bytes());
                offset += 8;
            }
        }

        // len check
        assert_eq!(offset, bitmap_extension_data.len());
        assert_eq!(
            bitmap_extension_data.len(),
            core::mem::size_of::<TickArrayBitmapExtension>() + 8
        );

        // deserialize original data
        let unpack_data: &TickArrayBitmapExtension = bytemuck::from_bytes(
            &bitmap_extension_data[8..core::mem::size_of::<TickArrayBitmapExtension>() + 8],
        );

        // data check
        let unpack_pool_id = unpack_data.pool_id;
        assert_eq!(unpack_pool_id, pool_id);
        for i in 0..EXTENSION_TICKARRAY_BITMAP_SIZE {
            for j in 0..8 {
                let unpack_positive_tick_array_bitmap =
                    unpack_data.positive_tick_array_bitmap[i][j];
                assert_eq!(
                    unpack_positive_tick_array_bitmap,
                    positive_tick_array_bitmap[i][j]
                );

                let unpack_negative_tick_array_bitmap =
                    unpack_data.negative_tick_array_bitmap[i][j];
                assert_eq!(
                    unpack_negative_tick_array_bitmap,
                    negative_tick_array_bitmap[i][j]
                );
            }
        }
    }
}
