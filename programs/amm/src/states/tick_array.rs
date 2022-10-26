use super::pool::PoolState;
use crate::error::ErrorCode;
use crate::libraries::{liquidity_math, tick_math};
use crate::pool::{RewardInfo, REWARD_NUM};
use crate::util::*;
use crate::Result;
use anchor_lang::{error::ErrorCode as anchorErrorCode, prelude::*, system_program};
use arrayref::array_ref;
use std::cell::RefMut;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::DerefMut;

pub const TICK_ARRAY_SEED: &str = "tick_array";
pub const TICK_ARRAY_SIZE_USIZE: usize = 60;
pub const TICK_ARRAY_SIZE: i32 = 60;
pub const MIN_TICK_ARRAY_START_INDEX: i32 = -307200;
pub const MAX_TICK_ARRAY_START_INDEX: i32 = 306600;

#[account(zero_copy)]
#[repr(packed)]
pub struct TickArrayState {
    pub pool_id: Pubkey,
    pub start_tick_index: i32,
    pub ticks: [TickState; TICK_ARRAY_SIZE_USIZE],
    pub initialized_tick_count: u8,
    // Unused bytes for future upgrades.
    pub padding: [u8; 115],
}

impl TickArrayState {
    pub const LEN: usize = 8 + 32 + 4 + TickState::LEN * TICK_ARRAY_SIZE_USIZE + 1 + 115;

    fn discriminator() -> [u8; 8] {
        [192, 155, 85, 205, 49, 249, 129, 42]
    }

    pub fn load_mut<'a>(account_info: &'a AccountInfo) -> Result<RefMut<'a, Self>> {
        if account_info.owner != &crate::id() {
            return Err(Error::from(anchorErrorCode::AccountOwnedByWrongProgram)
                .with_pubkeys((*account_info.owner, crate::id())));
        }
        if !account_info.is_writable {
            return Err(anchorErrorCode::AccountNotMutable.into());
        }
        require_eq!(account_info.data_len(), TickArrayState::LEN);

        let data = account_info.try_borrow_mut_data()?;
        let disc_bytes = array_ref![data, 0, 8];
        if disc_bytes != &TickArrayState::discriminator() {
            return Err(anchorErrorCode::AccountDiscriminatorMismatch.into());
        }
        Ok(RefMut::map(data, |data| {
            bytemuck::from_bytes_mut(
                &mut data.deref_mut()[8..std::mem::size_of::<TickArrayState>() + 8],
            )
        }))
    }

    /// Load a TickArrayState of type AccountLoader from tickarray account info, if tickarray account is not exist, then create it.
    pub fn get_or_create_tick_array<'info>(
        payer: AccountInfo<'info>,
        tick_array_account_info: AccountInfo<'info>,
        system_program: AccountInfo<'info>,
        pool_state_loader: &AccountLoader<'info, PoolState>,
        tick_array_start_index: i32,
        tick_spacing: u16,
    ) -> Result<AccountLoader<'info, TickArrayState>> {
        let tick_array_state = if tick_array_account_info.owner == &system_program::ID {
            let (expect_pda_address, bump) = Pubkey::find_program_address(
                &[
                    TICK_ARRAY_SEED.as_bytes(),
                    pool_state_loader.key().as_ref(),
                    &tick_array_start_index.to_be_bytes(),
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
                    pool_state_loader.key().as_ref(),
                    &tick_array_start_index.to_be_bytes(),
                    &[bump],
                ],
                TickArrayState::LEN,
            )?;
            let tick_array_state_loader = AccountLoader::<TickArrayState>::try_from_unchecked(
                &crate::id(),
                &tick_array_account_info,
            )?;
            {
                let mut tick_array_account = tick_array_state_loader.load_init()?;
                tick_array_account.initialize(
                    tick_array_start_index,
                    tick_spacing,
                    pool_state_loader.key(),
                )?;
            }
            // save the 8 byte discriminator
            tick_array_state_loader.exit(&crate::id())?;
            tick_array_state_loader
        } else {
            AccountLoader::<TickArrayState>::try_from(&tick_array_account_info)?
        };
        Ok(tick_array_state)
    }

    /**
     * Initialize only can be called when first created
     */
    pub fn initialize(
        &mut self,
        start_index: i32,
        tick_spacing: u16,
        pool_key: Pubkey,
    ) -> Result<()> {
        require_eq!(0, start_index % (TICK_ARRAY_SIZE * (tick_spacing) as i32));
        self.start_tick_index = start_index;
        self.pool_id = pool_key;
        Ok(())
    }

    pub fn update_initialized_tick_count(&mut self, add: bool) -> Result<()> {
        if add {
            self.initialized_tick_count += 1;
        } else {
            self.initialized_tick_count -= 1;
        }
        Ok(())
    }

    pub fn get_tick_state_mut(
        &mut self,
        tick_index: i32,
        tick_spacing: i32,
    ) -> Result<&mut TickState> {
        let offset_in_array = self.get_tick_offset_in_array(tick_index, tick_spacing)?;
        Ok(&mut self.ticks[offset_in_array])
    }

    pub fn update_tick_state(
        &mut self,
        tick_index: i32,
        tick_spacing: i32,
        tick_state: TickState,
    ) -> Result<()> {
        let offset_in_array = self.get_tick_offset_in_array(tick_index, tick_spacing)?;
        self.ticks[offset_in_array] = tick_state;
        Ok(())
    }

    /// Get tick's offset in current tick array, tick must be include in tick array， otherwise throw an error
    fn get_tick_offset_in_array(self, tick_index: i32, tick_spacing: i32) -> Result<usize> {
        require_eq!(0, tick_index % tick_spacing);
        let start_tick_index = TickArrayState::get_arrary_start_index(tick_index, tick_spacing);
        require_eq!(
            start_tick_index,
            self.start_tick_index,
            ErrorCode::InvalidTickArray
        );
        let offset_in_array = ((tick_index - self.start_tick_index) / tick_spacing) as usize;
        Ok(offset_in_array)
    }

    /// Base on swap directioin, return the first initialized tick in the tick array.
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

    /// Get next initialized tick in tick array, `current_tick_index` can be any tick index, in other words, `current_tick_index` not exactly a point in the tickarray,
    /// and current_tick_index % tick_spacing maybe not equal zero.
    /// If price move to left tick <= current_tick_index, or to right tick > current_tick_index
    pub fn next_initialized_tick(
        &mut self,
        current_tick_index: i32,
        tick_spacing: u16,
        zero_for_one: bool,
    ) -> Result<Option<&mut TickState>> {
        let current_tick_array_start_index =
            TickArrayState::get_arrary_start_index(current_tick_index, tick_spacing as i32);
        if current_tick_array_start_index != self.start_tick_index {
            let tick_state = self.first_initialized_tick(zero_for_one)?;
            return Ok(Some(tick_state));
        }
        let mut offset_in_array =
            (current_tick_index - self.start_tick_index) / (tick_spacing as i32);

        if zero_for_one {
            while offset_in_array >= 0 {
                if self.ticks[offset_in_array as usize].is_initialized() {
                    return Ok(self.ticks.get_mut(offset_in_array as usize));
                }
                offset_in_array = offset_in_array - 1;
            }
        } else {
            offset_in_array = offset_in_array + 1;
            while offset_in_array < TICK_ARRAY_SIZE {
                if self.ticks[offset_in_array as usize].is_initialized() {
                    return Ok(self.ticks.get_mut(offset_in_array as usize));
                }
                offset_in_array = offset_in_array + 1;
            }
        }
        Ok(None)
    }

    /// Base on swap directioin, return the next tick array start index.
    pub fn next_tick_arrary_start_index(&self, tick_spacing: u16, zero_for_one: bool) -> i32 {
        if zero_for_one {
            self.start_tick_index - (tick_spacing as i32) * TICK_ARRAY_SIZE
        } else {
            self.start_tick_index + (tick_spacing as i32) * TICK_ARRAY_SIZE
        }
    }

    /// Input an arbitrary tick_index, output the start_index of the tick_array it sits on
    pub fn get_arrary_start_index(tick_index: i32, tick_spacing: i32) -> i32 {
        let mut start = tick_index / (tick_spacing * TICK_ARRAY_SIZE);
        if tick_index < 0 && tick_index % (tick_spacing * TICK_ARRAY_SIZE) != 0 {
            start = start - 1
        }
        start * (tick_spacing * TICK_ARRAY_SIZE)
    }
}

impl Default for TickArrayState {
    #[inline]
    fn default() -> TickArrayState {
        TickArrayState {
            pool_id: Pubkey::default(),
            ticks: [TickState::default(); TICK_ARRAY_SIZE_USIZE],
            start_tick_index: 0,
            initialized_tick_count: 0,
            padding: [0; 115],
        }
    }
}

#[zero_copy]
#[repr(packed)]
#[derive(Default, Debug)]
pub struct TickState {
    pub tick: i32,
    /// Amount of net liquidity added (subtracted) when tick is crossed from left to right (right to left)
    pub liquidity_net: i128,
    /// The total position liquidity that references this tick
    pub liquidity_gross: u128,

    /// Fee growth per unit of liquidity on the _other_ side of this tick (relative to the current tick)
    /// only has relative meaning, not absolute — the value depends on when the tick is initialized
    pub fee_growth_outside_0_x64: u128,
    pub fee_growth_outside_1_x64: u128,

    // Reward growth per unit of liquidity like fee, array of Q64.64
    pub reward_growths_outside_x64: [u128; REWARD_NUM],
    // Unused bytes for future upgrades.
    pub padding: [u32; 13],
    // pub cross_up_liquidity_delta: u128,
    // pub cross_down_liquidity_delta: u128,
    // pub range_order_cross_up_time: u64,
    // pub range_order_cross_down_time: u64,
    // pub padding: u32,
}

impl TickState {
    pub const LEN: usize = 4 + 16 + 16 + 16 + 16 + 16 * REWARD_NUM + 16 + 16 + 8 + 8 + 4;

    pub fn initialize(&mut self, tick: i32, tick_spacing: u16) -> Result<()> {
        check_tick_boundary(tick, tick_spacing)?;
        self.tick = tick;
        Ok(())
    }
    /// Updates a tick and returns true if the tick was flipped from initialized to uninitialized
    pub fn update(
        &mut self,
        tick_current: i32,
        liquidity_delta: i128,
        fee_growth_global_0_x64: u128,
        fee_growth_global_1_x64: u128,
        upper: bool,
        reward_infos: &[RewardInfo; REWARD_NUM],
    ) -> Result<bool> {
        let liquidity_gross_before = self.liquidity_gross;
        let liquidity_gross_after =
            liquidity_math::add_delta(liquidity_gross_before, liquidity_delta)?;

        // Either liquidity_gross_after becomes 0 (uninitialized) XOR liquidity_gross_before
        // was zero (initialized)
        let flipped = (liquidity_gross_after == 0) != (liquidity_gross_before == 0);
        if liquidity_gross_before == 0 {
            // by convention, we assume that all growth before a tick was initialized happened _below_ the tick
            if self.tick <= tick_current {
                self.fee_growth_outside_0_x64 = fee_growth_global_0_x64;
                self.fee_growth_outside_1_x64 = fee_growth_global_1_x64;
                self.reward_growths_outside_x64 = RewardInfo::get_reward_growths(reward_infos);
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
}

// Calculates the fee growths inside of tick_lower and tick_upper based on their positions relative to tick_current.
/// `fee_growth_inside = fee_growth_global - fee_growth_below(lower) - fee_growth_above(upper)`
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
        .wrapping_sub(fee_growth_below_0_x64)
        .wrapping_sub(fee_growth_above_0_x64);
    let fee_growth_inside_1_x64 = fee_growth_global_1_x64
        .wrapping_sub(fee_growth_below_1_x64)
        .wrapping_sub(fee_growth_above_1_x64);

    (fee_growth_inside_0_x64, fee_growth_inside_1_x64)
}

// Calculates the reward growths inside of tick_lower and tick_upper based on their positions relative to tick_current.
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

        let reward_growths_below = if tick_current_index >= tick_lower.tick {
            tick_lower.reward_growths_outside_x64[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(tick_lower.reward_growths_outside_x64[i])
                .unwrap()
        };

        let reward_growths_above = if tick_current_index < tick_upper.tick {
            tick_upper.reward_growths_outside_x64[i]
        } else {
            reward_infos[i]
                .reward_growth_global_x64
                .checked_sub(tick_upper.reward_growths_outside_x64[i])
                .unwrap()
        };
        reward_growths_inside[i] = reward_infos[i]
            .reward_growth_global_x64
            .wrapping_sub(reward_growths_below)
            .wrapping_sub(reward_growths_above);
        #[cfg(feature = "enable-log")]
        msg!(
            "get_reward_growths_inside,i:{},reward_growth_global:{},reward_growth_below:{},reward_growth_above:{}, reward_growth_inside:{}",
            i,
            identity(reward_infos[i].reward_growth_global_x64),
            reward_growths_below,
            reward_growths_above,
            reward_growths_inside[i]
        );
    }

    reward_growths_inside
}

/// Common checks for a valid tick input.
/// A tick is valid iff it lies within tick boundaries and it is a multiple
/// of tick spacing.
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

pub fn check_tick_array_start_index(
    tick_array_start_index: i32,
    tick_index: i32,
    tick_spacing: u16,
) -> Result<()> {
    check_tick_boundary(tick_index, tick_spacing)?;
    let expect_start_index =
        TickArrayState::get_arrary_start_index(tick_index, tick_spacing as i32);
    require_eq!(tick_array_start_index, expect_start_index);
    assert!(
        tick_array_start_index >= tick_math::MIN_TICK
            && tick_array_start_index <= tick_math::MAX_TICK
    );
    Ok(())
}

/// Common checks for valid tick inputs.
///
pub fn check_ticks_order(tick_lower_index: i32, tick_upper_index: i32) -> Result<()> {
    require!(
        tick_lower_index < tick_upper_index,
        ErrorCode::TickInvaildOrder
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    mod tick_array_test {

        use super::*;
        use std::cell::RefCell;
        use std::convert::identity;

        fn build_new_tick_array(
            start_index: i32,
            tick_spacing: u16,
            initialized_tick_offsets: Vec<usize>,
        ) -> RefCell<TickArrayState> {
            let mut new_tick_array = TickArrayState::default();
            new_tick_array
                .initialize(start_index, tick_spacing, Pubkey::default())
                .unwrap();

            for offset in initialized_tick_offsets {
                let mut new_tick = TickState::default();
                // Indicates tick is initialized
                new_tick.liquidity_gross = 1;
                new_tick.tick = start_index + (offset * tick_spacing as usize) as i32;
                new_tick_array.ticks[offset] = new_tick;
            }
            RefCell::new(new_tick_array)
        }

        #[test]
        fn get_arrary_start_index_test() {
            assert_eq!(TickArrayState::get_arrary_start_index(120, 3), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(1002, 30), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(-120, 3), -180);
            assert_eq!(TickArrayState::get_arrary_start_index(-1002, 30), -1800);
            assert_eq!(TickArrayState::get_arrary_start_index(-20, 10), -600);
            assert_eq!(TickArrayState::get_arrary_start_index(20, 10), 0);
            assert_eq!(TickArrayState::get_arrary_start_index(-1002, 10), -1200);
            assert_eq!(TickArrayState::get_arrary_start_index(-600, 10), -600);
        }

        #[test]
        fn next_tick_arrary_start_index_test() {
            let tick_spacing = 15;
            let tick_array_ref = build_new_tick_array(-1800, tick_spacing, vec![]);
            assert_eq!(
                -2700,
                tick_array_ref
                    .borrow()
                    .next_tick_arrary_start_index(tick_spacing, true)
            );
            assert_eq!(
                -900,
                tick_array_ref
                    .borrow()
                    .next_tick_arrary_start_index(tick_spacing, false)
            );
        }

        #[test]
        fn get_tick_offset_in_array_test() {
            let tick_spacing = 4;
            let tick_array_ref = build_new_tick_array(960, tick_spacing, vec![]);
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(1105, 4)
                    .unwrap_err(),
                error!(anchor_lang::error::ErrorCode::RequireEqViolated)
            );
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(808, 4)
                    .unwrap_err(),
                error!(ErrorCode::InvalidTickArray)
            );
            assert_eq!(
                tick_array_ref
                    .borrow()
                    .get_tick_offset_in_array(1108, 4)
                    .unwrap(),
                37
            );
        }

        #[test]
        fn first_initialized_tick_test() {
            let tick_spacing = 15;
            let tick_array_ref = build_new_tick_array(-900, tick_spacing, vec![40, 59]);
            let mut tick_array = tick_array_ref.borrow_mut();

            let tick = tick_array.first_initialized_tick(false).unwrap().tick;
            assert_eq!(-300, tick);

            let tick = tick_array.first_initialized_tick(true).unwrap().tick;
            assert_eq!(-15, tick);
        }

        #[test]
        fn next_initialized_tick_when_tick_is_positive() {
            // init tick_index [0,30,105]
            let tick_array_ref = build_new_tick_array(0, 15, vec![0, 2, 7]);
            let mut tick_array = tick_array_ref.borrow_mut();

            // test zero_for_one
            let mut next_tick_state = tick_array.next_initialized_tick(0, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);

            next_tick_state = tick_array.next_initialized_tick(1, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);

            next_tick_state = tick_array.next_initialized_tick(29, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 0);
            next_tick_state = tick_array.next_initialized_tick(30, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);
            next_tick_state = tick_array.next_initialized_tick(31, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);

            // test one for zero
            let mut next_tick_state = tick_array.next_initialized_tick(0, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);

            next_tick_state = tick_array.next_initialized_tick(29, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 30);
            next_tick_state = tick_array.next_initialized_tick(30, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 105);
            next_tick_state = tick_array.next_initialized_tick(31, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), 105);

            next_tick_state = tick_array.next_initialized_tick(105, 15, false).unwrap();
            assert!(next_tick_state.is_none());
        }

        #[test]
        fn next_initialized_tick_when_tick_is_negative() {
            // init tick_index [-900,-870,-795]
            let tick_array_ref = build_new_tick_array(-900, 15, vec![0, 2, 7]);
            let mut tick_array = tick_array_ref.borrow_mut();

            // test zero for one
            let mut next_tick_state = tick_array.next_initialized_tick(-900, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);

            next_tick_state = tick_array.next_initialized_tick(-899, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);

            next_tick_state = tick_array.next_initialized_tick(-871, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -900);
            next_tick_state = tick_array.next_initialized_tick(-870, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);
            next_tick_state = tick_array.next_initialized_tick(-869, 15, true).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);

            // test one for zero
            let mut next_tick_state = tick_array.next_initialized_tick(-900, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);

            next_tick_state = tick_array.next_initialized_tick(-871, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -870);
            next_tick_state = tick_array.next_initialized_tick(-870, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -795);
            next_tick_state = tick_array.next_initialized_tick(-869, 15, false).unwrap();
            assert_eq!(identity(next_tick_state.unwrap().tick), -795);

            next_tick_state = tick_array.next_initialized_tick(-795, 15, false).unwrap();
            assert!(next_tick_state.is_none());
        }
    }

    mod get_fee_growth_inside_test {
        use crate::states::{
            pool::RewardInfo,
            tick_array::{get_fee_growth_inside, TickState},
        };

        /// Position tick range is below current tick
        #[test]
        fn position_in_left_side() {
            let tick_current = 0;
            let fee_growth_global_0_x64 = 0;
            let fee_growth_global_1_x64 = 1000;

            let tick_lower = &mut TickState {
                tick: -10,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: -5,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };

            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 0;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 340282366920938463463374607431768210456);

            tick_lower.fee_growth_outside_1_x64 = 0;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 1000);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);
        }

        /// Current tick between position tick range
        #[test]
        fn position_in_range() {
            let mut tick_current = 0;
            let fee_growth_global_0_x64 = 0;
            let mut fee_growth_global_1_x64 = 1000;

            let tick_lower = &mut TickState {
                tick: -10,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };

            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 1000);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 0;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);

            tick_lower.fee_growth_outside_1_x64 = 0;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 340282366920938463463374607431768210456);

            fee_growth_global_1_x64 = 1200;
            tick_current = 9;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 340282366920938463463374607431768210656);

            fee_growth_global_1_x64 = 1500;
            tick_upper.cross(0, fee_growth_global_1_x64, &[RewardInfo::default(); 3]);
            tick_current = 11;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 340282366920938463463374607431768210956);
        }

        /// Position tick range is above current tick
        #[test]
        fn position_in_right_side() {
            let tick_current = 0;
            let fee_growth_global_0_x64 = 0;
            let fee_growth_global_1_x64 = 1000;

            let tick_lower = &mut TickState {
                tick: 1,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                fee_growth_outside_0_x64: 0,
                fee_growth_outside_1_x64: 0,
                ..Default::default()
            };

            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 0;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 1000);

            tick_lower.fee_growth_outside_1_x64 = 0;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 340282366920938463463374607431768210456);

            tick_lower.fee_growth_outside_1_x64 = 1000;
            tick_upper.fee_growth_outside_1_x64 = 1000;
            let (fee_growth_inside_0, fee_growth_inside_1) = get_fee_growth_inside(
                tick_lower,
                tick_upper,
                tick_current,
                fee_growth_global_0_x64,
                fee_growth_global_1_x64,
            );
            assert_eq!(fee_growth_inside_0, 0);
            assert_eq!(fee_growth_inside_1, 0);
        }
    }

    mod get_reward_growths_inside_test {
        use std::str::FromStr;

        use anchor_lang::prelude::Pubkey;

        use crate::states::{
            pool::RewardInfo,
            tick_array::{get_reward_growths_inside, TickState},
        };

        #[test]
        fn uninitialized_reward_index_test() {
            let tick_current = 0;

            let tick_lower = &mut TickState {
                tick: -10,
                reward_growths_outside_x64: [1000, 0, 0],
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                reward_growths_outside_x64: [1000, 0, 0],
                ..Default::default()
            };

            let reward_infos = &[RewardInfo::default(); 3];
            let reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current, reward_infos);
            assert_eq!(reward_inside, [0; 3]);
        }

        /// Position tick range is below current tick
        #[test]
        fn position_in_left_side() {
            let tick_current_index = 0;
            let tick_lower = &mut TickState {
                tick: -10,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: -5,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };

            let reward_infos = &mut [
                RewardInfo {
                    token_mint: Pubkey::from_str("So11111111111111111111111111111111111111112")
                        .unwrap(),
                    reward_growth_global_x64: 1000,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ];

            let mut reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(
                reward_inside,
                [340282366920938463463374607431768210456, 0, 0]
            );

            tick_lower.reward_growths_outside_x64 = [0, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [1000, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);
        }

        /// Current tick between position tick range
        #[test]
        fn position_in_range() {
            let mut tick_current_index = 0;

            let tick_lower = &mut TickState {
                tick: -10,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };

            let reward_infos = &mut [
                RewardInfo {
                    token_mint: Pubkey::from_str("So11111111111111111111111111111111111111112")
                        .unwrap(),
                    reward_growth_global_x64: 1000,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ];

            let mut reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [1000, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [0, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(
                reward_inside,
                [340282366920938463463374607431768210456, 0, 0]
            );

            let reward_infos = &mut [
                RewardInfo {
                    token_mint: Pubkey::from_str("So11111111111111111111111111111111111111112")
                        .unwrap(),
                    reward_growth_global_x64: 1200,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ];
            tick_current_index = 9;
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(
                reward_inside,
                [340282366920938463463374607431768210656, 0, 0]
            );

            let reward_infos = &mut [
                RewardInfo {
                    token_mint: Pubkey::from_str("So11111111111111111111111111111111111111112")
                        .unwrap(),
                    reward_growth_global_x64: 1500,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ];
            tick_upper.cross(0, 0, reward_infos);
            tick_current_index = 11;
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(
                reward_inside,
                [340282366920938463463374607431768210956, 0, 0]
            );
        }

        /// Position tick range is above current tick
        #[test]
        fn position_in_right_side() {
            let tick_current_index = 0;

            let tick_lower = &mut TickState {
                tick: 1,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };
            let tick_upper = &mut TickState {
                tick: 10,
                reward_growths_outside_x64: [0, 0, 0],
                ..Default::default()
            };

            let reward_infos = &mut [
                RewardInfo {
                    token_mint: Pubkey::from_str("So11111111111111111111111111111111111111112")
                        .unwrap(),
                    reward_growth_global_x64: 1000,
                    ..Default::default()
                },
                RewardInfo::default(),
                RewardInfo::default(),
            ];

            let mut reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [1000, 0, 0]);

            tick_lower.reward_growths_outside_x64 = [0, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(
                reward_inside,
                [340282366920938463463374607431768210456, 0, 0]
            );

            tick_lower.reward_growths_outside_x64 = [1000, 0, 0];
            tick_upper.reward_growths_outside_x64 = [1000, 0, 0];
            reward_inside =
                get_reward_growths_inside(tick_lower, tick_upper, tick_current_index, reward_infos);
            assert_eq!(reward_inside, [0, 0, 0]);
        }
    }
}
