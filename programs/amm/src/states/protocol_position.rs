use crate::libraries::tick_math;
use crate::libraries::{big_num::U128, full_math::MulDiv};
use crate::pool::REWARD_NUM;
use crate::util::get_recent_epoch;
use crate::{
    error::ErrorCode,
    libraries::{fixed_point_64, liquidity_math},
};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const POSITION_SEED: &str = "position";

/// Info stored for each user's position
#[account]
#[derive(Default, Debug)]
pub struct ProtocolPositionState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The lower bound tick of the position
    pub tick_lower_index: i32,

    /// The upper bound tick of the position
    pub tick_upper_index: i32,

    /// The amount of liquidity owned by this position
    pub liquidity: u128,

    /// The token_0 fee growth per unit of liquidity as of the last update to liquidity or fees owed
    pub fee_growth_inside_0_last_x64: u128,

    /// The token_1 fee growth per unit of liquidity as of the last update to liquidity or fees owed
    pub fee_growth_inside_1_last_x64: u128,

    /// The fees owed to the position owner in token_0
    pub token_fees_owed_0: u64,

    /// The fees owed to the position owner in token_1
    pub token_fees_owed_1: u64,

    /// The reward growth per unit of liquidity as of the last update to liquidity
    pub reward_growth_inside: [u128; REWARD_NUM], // 24
    // account update recent epoch
    pub recent_epoch: u64,
    // Unused bytes for future upgrades.
    pub padding: [u64; 7],
}

impl ProtocolPositionState {
    pub const LEN: usize = 8 + 1 + 32 + 4 + 4 + 16 + 16 + 16 + 8 + 8 + 16 * REWARD_NUM + 64;

    pub fn update(
        &mut self,
        tick_lower_index: i32,
        tick_upper_index: i32,
        liquidity_delta: i128,
        fee_growth_inside_0_x64: u128,
        fee_growth_inside_1_x64: u128,
        reward_growths_inside: [u128; REWARD_NUM],
    ) -> Result<()> {
        if self.liquidity == 0 && liquidity_delta == 0 {
            return Ok(());
        }
        require!(
            tick_lower_index >= tick_math::MIN_TICK && tick_lower_index <= tick_math::MAX_TICK,
            ErrorCode::TickLowerOverflow
        );
        require!(
            tick_upper_index >= tick_math::MIN_TICK && tick_upper_index <= tick_math::MAX_TICK,
            ErrorCode::TickUpperOverflow
        );
        // calculate accumulated Fees
        let tokens_owed_0 =
            U128::from(fee_growth_inside_0_x64.saturating_sub(self.fee_growth_inside_0_last_x64))
                .mul_div_floor(U128::from(self.liquidity), U128::from(fixed_point_64::Q64))
                .unwrap()
                .to_underflow_u64();
        let tokens_owed_1 =
            U128::from(fee_growth_inside_1_x64.saturating_sub(self.fee_growth_inside_1_last_x64))
                .mul_div_floor(U128::from(self.liquidity), U128::from(fixed_point_64::Q64))
                .unwrap()
                .to_underflow_u64();

        // Update the position liquidity
        self.liquidity = liquidity_math::add_delta(self.liquidity, liquidity_delta)?;
        self.fee_growth_inside_0_last_x64 = fee_growth_inside_0_x64;
        self.fee_growth_inside_1_last_x64 = fee_growth_inside_1_x64;
        self.tick_lower_index = tick_lower_index;
        self.tick_upper_index = tick_upper_index;
        if tokens_owed_0 > 0 || tokens_owed_1 > 0 {
            self.token_fees_owed_0 = self.token_fees_owed_0.checked_add(tokens_owed_0).unwrap();
            self.token_fees_owed_1 = self.token_fees_owed_1.checked_add(tokens_owed_1).unwrap();
        }
        #[cfg(feature = "enable-log")]
        msg!(
            "protocol position reward_growths_inside:{:?}",
            reward_growths_inside
        );
        self.update_reward_growths_inside(reward_growths_inside);
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }

    pub fn update_reward_growths_inside(&mut self, reward_growths_inside: [u128; REWARD_NUM]) {
        // just record, calculate reward owed in personal position
        self.reward_growth_inside = reward_growths_inside;
    }
}
