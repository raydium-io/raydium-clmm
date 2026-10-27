use crate::instructions::initialize_reward::{
    initialize_reward_internal, InitializeReward, InitializeRewardParam,
};
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `initialize_reward` with a declared account tape, and admission for hook reward mints.
///
/// The reward slot is chosen inside the handler, so the funding transfer's hook leg is
/// labelled `TransferHookReward0` regardless of which slot it lands in.
pub fn initialize_reward_v2<'info>(
    ctx: Context<'info, InitializeReward<'info>>,
    param: InitializeRewardParam,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;

    initialize_reward_internal(ctx, parsed.mint_admission, parsed.hook_reward[0], param)
}
