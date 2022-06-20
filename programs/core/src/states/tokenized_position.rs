use crate::pool::NUM_REWARDS;
use anchor_lang::prelude::*;
/// Position wrapped as an SPL non-fungible token
///
/// PDA of `[POSITION_SEED, mint_address]`
///
#[account]
#[derive(Default)]
pub struct PersonalPositionState {
    /// Bump to identify PDA
    pub bump: u8,

    /// Mint address of the tokenized position
    pub mint: Pubkey,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The lower bound tick of the position
    pub tick_lower: i32,

    /// The upper bound tick of the position
    pub tick_upper: i32,

    /// The amount of liquidity owned by this position
    pub liquidity: u64,

    /// The token_0 fee growth of the aggregate position as of the last action on the individual position
    pub fee_growth_inside_0_last: u64,

    /// The token_0 fee growth of the aggregate position as of the last action on the individual position
    pub fee_growth_inside_1_last: u64,

    /// How many uncollected token_0 are owed to the position, as of the last computation
    pub tokens_owed_0: u64,

    /// How many uncollected token_1 are owed to the position, as of the last computation
    pub tokens_owed_1: u64,

    // Position reward info
    pub reward_infos: [PositionRewardInfo; NUM_REWARDS],
    // padding space for upgrade
    // pub padding: [u64; 8],
}

impl PersonalPositionState {
    pub const LEN: usize = 8 + 1 + 32 + 32 + 4 + 4 + 8 + 8 + 8 + 8 + 8 + 16 * NUM_REWARDS + 64;

    pub fn update_rewards(&mut self, reward_growths_inside: [u64; NUM_REWARDS]) -> Result<()> {
        for i in 0..NUM_REWARDS {
            let reward_growth_inside = reward_growths_inside[i];
            let curr_reward_info = self.reward_infos[i];

            // Calculate reward delta.
            // If reward delta overflows, default to a zero value. This means the position loses all
            // rewards earned since the last time the position was modified or rewards were collected.
            let reward_growth_delta = reward_growth_inside
                .checked_sub(curr_reward_info.growth_inside_last)
                .unwrap_or(0);
            let amount_owed_delta = self.liquidity.checked_mul(reward_growth_delta).unwrap();

            self.reward_infos[i].growth_inside_last = reward_growth_inside;

            // Overflows allowed. Must collect rewards owed before overflow.
            self.reward_infos[i].reward_amount_owed = curr_reward_info
                .reward_amount_owed
                .checked_add(amount_owed_delta)
                .unwrap();
        }
        Ok(())
    }
}

#[derive(Copy, Clone, AnchorSerialize, AnchorDeserialize, Default, Debug, PartialEq)]
pub struct PositionRewardInfo {
    pub growth_inside_last: u64,
    pub reward_amount_owed: u64,
}

/// Emitted when liquidity is increased for a position NFT.
/// Also emitted when a token is minted
#[event]
pub struct IncreaseLiquidityEvent {
    /// The ID of the token for which liquidity was increased
    #[index]
    pub position_nft_mint: Pubkey,

    /// The amount by which liquidity for the NFT position was increased
    pub liquidity: u64,

    /// The amount of token_0 that was paid for the increase in liquidity
    pub amount_0: u64,

    /// The amount of token_1 that was paid for the increase in liquidity
    pub amount_1: u64,
}

/// Emitted when liquidity is decreased for a position NFT
#[event]
pub struct DecreaseLiquidityEvent {
    /// The ID of the token for which liquidity was decreased
    #[index]
    pub position_nft_mint: Pubkey,

    /// The amount by which liquidity for the NFT position was decreased
    pub liquidity: u64,

    /// The amount of token_0 that was accounted for the decrease in liquidity
    pub amount_0: u64,

    /// The amount of token_1 that was accounted for the decrease in liquidity
    pub amount_1: u64,
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
pub struct UpdateFeeAndRewardsEvent {
    /// The ID of the token for which underlying tokens were collected
    #[index]
    pub position_nft_mint: Pubkey,
    /// The amount of token_0 owed to the position that was collected
    pub tokens_owed_0: u64,
    /// The amount of token_1 owed to the position that was collected
    pub tokens_owed_1: u64,
    /// Reward info
    pub reward_infos: [PositionRewardInfo; NUM_REWARDS],
}

/// Emitted when Reward are collected for a position NFT
#[event]
pub struct CollectRewardEvent {
    /// The ID of the token for which underlying tokens were collected
    #[index]
    pub reward_mint: Pubkey,
    /// The amount of token_0 owed to the position that was collected
    pub reward_amount: u64,
    /// The amount of token_1 owed to the position that was collected
    pub reward_index: u8,
}
