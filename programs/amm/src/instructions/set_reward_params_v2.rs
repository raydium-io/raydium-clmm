use crate::error::ErrorCode;
use crate::instructions::set_reward_params::{set_reward_params_internal, SetRewardParams};
use crate::states::pool::REWARD_NUM;
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `set_reward_params` with a declared account tape.
///
/// The {vault, authority token account, mint} triple used to be read off the front of
/// `remaining_accounts` only when the update implied a top-up — a conditional offset the
/// caller had to reproduce. It is now its own `RewardGroup` segment.
pub fn set_reward_params_v2<'info>(
    ctx: Context<'info, SetRewardParams<'info>>,
    reward_index: u8,
    emissions_per_second_x64: u128,
    open_time: u64,
    end_time: u64,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    require!(
        (reward_index as usize) < REWARD_NUM,
        ErrorCode::InvalidRewardIndex
    );
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;

    let hook_reward = parsed.hook_reward[reward_index as usize];
    set_reward_params_internal(
        ctx,
        parsed.reward_group,
        hook_reward,
        true,
        reward_index,
        emissions_per_second_x64,
        open_time,
        end_time,
    )
}
