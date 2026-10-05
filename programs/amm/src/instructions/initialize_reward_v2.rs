use crate::instructions::initialize_reward::{
    initialize_reward_internal, InitializeReward, InitializeRewardParam,
};
use crate::util::{parse_remaining_accounts, AccountsType, RemainingAccountsInfo};
use anchor_lang::prelude::*;

/// `initialize_reward` with a declared account tape, and admission for hook reward mints.
///
/// A hook reward mint is admitted exactly like token_0 / token_1: it must be in
/// `SupportMintAssociated`, and its extras list must be small enough and amount-independent
/// enough that `collect_reward` can be assembled later.
///
/// The reward slot is chosen inside the handler, so the funding transfer's hook leg is
/// labelled `TransferHookReward0` regardless of which slot it lands in.
pub fn initialize_reward_v2<'info>(
    ctx: Context<'info, InitializeReward<'info>>,
    param: InitializeRewardParam,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    let parsed = parse_remaining_accounts(
        ctx.remaining_accounts,
        &remaining_accounts_info,
        &[
            AccountsType::MintAdmission,
            AccountsType::TransferHookReward0,
        ],
    )?;

    initialize_reward_internal(ctx, parsed.mint_admission, parsed.hook_reward[0], param)
}
