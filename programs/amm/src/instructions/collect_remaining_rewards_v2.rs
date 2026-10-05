use crate::error::ErrorCode;
use crate::instructions::collect_remaining_rewards::{
    collect_remaining_rewards_internal, CollectRemainingRewards,
};
use crate::states::pool::REWARD_NUM;
use crate::util::{parse_remaining_accounts, reward_hook_type, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `collect_remaining_rewards` with a declared account tape.
pub fn collect_remaining_rewards_v2<'info>(
    ctx: Context<'info, CollectRemainingRewards<'info>>,
    reward_index: u8,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    require!(
        (reward_index as usize) < REWARD_NUM,
        ErrorCode::InvalidRewardIndex
    );
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[reward_hook_type(reward_index)?],
    )?;

    collect_remaining_rewards_internal(ctx, parsed.hook_reward[reward_index as usize], reward_index)
}
