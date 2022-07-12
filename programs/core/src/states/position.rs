use crate::libraries::{big_num::U128, full_math::MulDiv};
use crate::pool::REWARD_NUM;
use crate::{
    error::ErrorCode,
    libraries::{fixed_point_64, liquidity_math},
};
///! Positions represent an owner address' liquidity between a lower and upper tick boundary
///! Positions store additional state for tracking fees owed to the position
///!
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const POSITION_SEED: &str = "position";

/// Info stored for each user's position
///
/// PDA of `[POSITION_SEED, token_0, token_1, fee, owner, tick_lower, tick_upper]`
///
#[account]
#[derive(Default, Debug)]
pub struct ProcotolPositionState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The amount of liquidity owned by this position
    pub liquidity: u128,

    /// The token_0 fee growth per unit of liquidity as of the last update to liquidity or fees owed
    pub fee_growth_inside_0_last: u128,

    /// The token_1 fee growth per unit of liquidity as of the last update to liquidity or fees owed
    pub fee_growth_inside_1_last: u128,

    /// The fees owed to the position owner in token_0
    pub token_fees_owed_0: u64,

    /// The fees owed to the position owner in token_1
    pub token_fees_owed_1: u64,

    /// The reward growth per unit of liquidity as of the last update to liquidity
    pub reward_growth_inside: [u128; REWARD_NUM], // 24
                                                  // padding space for upgrade
                                                  // pub padding: [u64; 8],
}

impl ProcotolPositionState {
    pub const LEN: usize = 8 + 1 + 16 + 16 + 16 + 8 + 8 + 16 * REWARD_NUM + 64;
    /// Credits accumulated fees to a user's position
    ///
    /// # Arguments
    ///
    /// * `self` - The individual position to update
    /// * `liquidity_delta` - The change in pool liquidity as a result of the position update
    /// * `fee_growth_inside_0_x64` - The all-time fee growth in token_0, per unit of liquidity,
    /// inside the position's tick boundaries
    /// * `fee_growth_inside_1_x64` - The all-time fee growth in token_1, per unit of liquidity,
    /// inside the position's tick boundaries
    ///
    pub fn update(
        &mut self,
        liquidity_delta: i128,
        fee_growth_inside_0_x64: u128,
        fee_growth_inside_1_x64: u128,
        reward_growths_inside: [u128; REWARD_NUM],
    ) -> Result<()> {
        let liquidity_next = if liquidity_delta == 0 {
            require!(self.liquidity > 0, ErrorCode::InvaildLiquidity); // disallow pokes for 0 liquidity positions
            self.liquidity
        } else {
            liquidity_math::add_delta(self.liquidity, liquidity_delta)?
        };

        // calculate accumulated Fees
        let tokens_owed_0 =
            U128::from(fee_growth_inside_0_x64.saturating_sub(self.fee_growth_inside_0_last))
                .mul_div_floor(U128::from(self.liquidity), U128::from(fixed_point_64::Q64))
                .unwrap()
                .as_u64();
        let tokens_owed_1 =
            U128::from(fee_growth_inside_1_x64.saturating_sub(self.fee_growth_inside_1_last))
                .mul_div_floor(U128::from(self.liquidity), U128::from(fixed_point_64::Q64))
                .unwrap()
                .as_u64();

        // Update the position
        if liquidity_delta != 0 {
            self.liquidity = liquidity_next;
        }
        self.fee_growth_inside_0_last = fee_growth_inside_0_x64;
        self.fee_growth_inside_1_last = fee_growth_inside_1_x64;
        if tokens_owed_0 > 0 || tokens_owed_1 > 0 {
            // overflow is acceptable, have to withdraw before you hit u64::MAX fees
            self.token_fees_owed_0 = self.token_fees_owed_0.checked_add(tokens_owed_0).unwrap();
            self.token_fees_owed_1 = self.token_fees_owed_1.checked_add(tokens_owed_1).unwrap();
        }

        self.update_reward_growths_inside(reward_growths_inside);
        Ok(())
    }

    pub fn update_reward_growths_inside(&mut self, reward_growths_inside: [u128; REWARD_NUM]) {
        // just record, calculate reward owed in persional position
        self.reward_growth_inside = reward_growths_inside;
    }
}

/// Emitted when liquidity is minted for a given position
#[event]
pub struct CreatePersonalPositionEvent {
    /// The pool for which liquidity was minted
    #[index]
    pub pool_state: Pubkey,

    /// The address that minted the liquidity
    pub minter: Pubkey,

    /// The owner of the position and recipient of any minted liquidity
    pub nft_owner: Pubkey,

    /// The lower tick of the position
    #[index]
    pub tick_lower: i32,

    /// The upper tick of the position
    #[index]
    pub tick_upper: i32,

    /// The amount of liquidity minted to the position range
    pub liquidity: u128,

    /// How much token_0 was required for the minted liquidity
    pub deposit_amount_0: u64,

    /// How much token_1 was required for the minted liquidity
    pub deposit_amount_1: u64,
}

/// Emitted when a position's liquidity is removed.
/// Does not withdraw any fees earned by the liquidity position, which must be withdrawn via #collect
#[event]
pub struct BurnEvent {
    /// The pool from where liquidity was removed
    #[index]
    pub pool_state: Pubkey,

    /// The owner of the position for which liquidity is removed
    pub owner: Pubkey,

    /// The lower tick of the position
    #[index]
    pub tick_lower: i32,

    /// The upper tick of the position
    #[index]
    pub tick_upper: i32,

    /// The amount of liquidity to remove
    pub amount: u64,

    /// The amount of token_0 withdrawn
    pub amount_0: u64,

    /// The amount of token_1 withdrawn
    pub amount_1: u64,
}

/// Emitted when fees are collected by the owner of a position
/// Collect events may be emitted with zero amount_0 and amount_1 when the caller chooses not to collect fees
#[event]
pub struct CollectFeeEvent {
    /// The pool from which fees are collected
    #[index]
    pub pool_state: Pubkey,

    /// The owner of the position for which fees are collected
    pub owner: Pubkey,

    /// The lower tick of the position
    #[index]
    pub tick_lower: i32,

    /// The upper tick of the position
    #[index]
    pub tick_upper: i32,

    /// The amount of token_0 fees collected
    pub collect_amount_0: u64,

    /// The amount of token_1 fees collected
    pub collect_amount_1: u64,
}
