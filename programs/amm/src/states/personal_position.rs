use super::pool::RewardInfo;
use crate::libraries::{big_num::U256, fixed_point_64, full_math::MulDiv};
use crate::pool::REWARD_NUM;
use anchor_lang::prelude::*;

#[account]
#[derive(Default, Debug)]
pub struct PersonalPositionState {
    /// Bump to identify PDA
    pub bump: u8,

    /// Mint address of the tokenized position
    pub nft_mint: Pubkey,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The lower bound tick of the position
    pub tick_lower_index: i32,

    /// The upper bound tick of the position
    pub tick_upper_index: i32,

    /// The amount of liquidity owned by this position
    pub liquidity: u128,

    /// The token_0 fee growth of the aggregate position as of the last action on the individual position
    pub fee_growth_inside_0_last_x64: u128,

    /// The token_1 fee growth of the aggregate position as of the last action on the individual position
    pub fee_growth_inside_1_last_x64: u128,

    /// The fees owed to the position owner in token_0, as of the last computation
    pub token_fees_owed_0: u64,

    /// The fees owed to the position owner in token_1, as of the last computation
    pub token_fees_owed_1: u64,

    // Position reward info
    pub reward_infos: [PositionRewardInfo; REWARD_NUM],
    // Unused bytes for future upgrades.
    pub padding: [u64; 8],
    // pub range_order_type: u8,
    // pub range_order_open_time: u64,
}

impl PersonalPositionState {
    pub const LEN: usize =
        8 + 1 + 32 + 32 + 4 + 4 + 16 + 16 + 16 + 8 + 8 + PositionRewardInfo::LEN * REWARD_NUM + 64;

    pub fn update_rewards(&mut self, reward_growths_inside: [u128; REWARD_NUM]) -> Result<()> {
        for i in 0..REWARD_NUM {
            let reward_growth_inside = reward_growths_inside[i];
            let curr_reward_info = self.reward_infos[i];

            // Calculate reward delta.
            // If reward delta overflows, default to a zero value. This means the position loses all
            // rewards earned since the last time the position was modified or rewards were collected.
            let reward_growth_delta = reward_growth_inside
                .saturating_sub (curr_reward_info.growth_inside_last_x64);

            let amount_owed_delta = U256::from(reward_growth_delta)
                .mul_div_floor(U256::from(self.liquidity), U256::from(fixed_point_64::Q64))
                .unwrap()
                .as_u64();
            self.reward_infos[i].growth_inside_last_x64 = reward_growth_inside;

            // Overflows allowed. Must collect rewards owed before overflow.
            self.reward_infos[i].reward_amount_owed = curr_reward_info
                .reward_amount_owed
                .checked_add(amount_owed_delta)
                .unwrap();
            #[cfg(feature = "enable-log")]
            msg!("update personal reward, index:{}, owed_before:{:?}, amount_owed_delta:{}, owed_after:{}, reward_growth_delta:{}, self.liquidity:{}", i, curr_reward_info.reward_amount_owed,amount_owed_delta, self.reward_infos[i].reward_amount_owed,reward_growth_delta,self.liquidity );
        }
        Ok(())
    }
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Default, Debug, PartialEq)]
pub struct PositionRewardInfo {
    // Q64.64
    pub growth_inside_last_x64: u128,
    pub reward_amount_owed: u64,
}

impl PositionRewardInfo {
    pub const LEN: usize = 16 + 8;
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
    pub tick_lower_index: i32,

    /// The upper tick of the position
    #[index]
    pub tick_upper_index: i32,

    /// The amount of liquidity minted to the position range
    pub liquidity: u128,

    /// How much token_0 was required for the minted liquidity
    pub deposit_amount_0: u64,

    /// How much token_1 was required for the minted liquidity
    pub deposit_amount_1: u64,
}

/// Emitted when liquidity is increased.
#[event]
pub struct IncreaseLiquidityEvent {
    /// The ID of the token for which liquidity was increased
    #[index]
    pub position_nft_mint: Pubkey,

    /// The amount by which liquidity for the NFT position was increased
    pub liquidity: u128,

    /// The amount of token_0 that was paid for the increase in liquidity
    pub amount_0: u64,

    /// The amount of token_1 that was paid for the increase in liquidity
    pub amount_1: u64,
}

/// Emitted when liquidity is decreased.
#[event]
pub struct DecreaseLiquidityEvent {
    /// The ID of the token for which liquidity was increased
    pub position_nft_mint: Pubkey,
    /// The amount by which liquidity for the NFT position was increased
    pub liquidity: u128,
    /// The amount of token_0 that was paid for the decrease in liquidity
    pub decrease_amount_0: u64,
    /// The amount of token_1 that was paid for the decrease in liquidity
    pub decrease_amount_1: u64,
    // The amount of token_0 fee
    pub fee_amount_0: u64,
    /// The amount of token_1 fee
    pub fee_amount_1: u64,
    /// The amount of rewards
    pub reward_amounts: [u64; REWARD_NUM],
}

/// Emitted when tokens are collected for a position NFT
/// The amounts reported may not be exactly equivalent to the amounts transferred, due to rounding behavior
#[event]
pub struct CollectPersonalFeeEvent {
    /// The ID of the token for which underlying tokens were collected
    #[index]
    pub position_nft_mint: Pubkey,

    /// The token account that received the collected token_0 tokens
    pub recipient_token_account_0: Pubkey,

    /// The token account that received the collected token_1 tokens
    pub recipient_token_account_1: Pubkey,

    /// The amount of token_0 owed to the position that was collected
    pub amount_0: u64,

    /// The amount of token_1 owed to the position that was collected
    pub amount_1: u64,
}

/// Emitted when Reward are updated for a position NFT
#[event]
pub struct UpdateRewardInfosEvent {
    /// Reward info
    pub reward_infos: [RewardInfo; REWARD_NUM],
}
