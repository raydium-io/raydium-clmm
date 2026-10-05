use crate::error::ErrorCode;
use crate::states::pool::REWARD_NUM;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::TokenAccount;

#[derive(Accounts)]
pub struct ForfeitReward<'info> {
    /// The position NFT holder, who is giving up the reward
    pub nft_owner: Signer<'info>,

    #[account(
        constraint = nft_account.mint == personal_position.nft_mint,
        constraint = nft_account.amount == 1,
        token::authority = nft_owner,
    )]
    pub nft_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut, constraint = personal_position.pool_id == pool_state.key())]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

/// Give up an uncollectable reward so the position can be closed.
///
/// `close_position` refuses while any `reward_amount_owed` is non-zero, so a reward that can
/// never be paid out - a hook that rejects the transfer, or a frozen vault - would strand the
/// position NFT and its rent forever.
///
/// No transfer happens: crediting `reward_claimed` instead makes the amount reclaimable by the
/// funder through `collect_remaining_rewards`, and keeps
/// `reward_total_emitted - reward_claimed >= reward_amount_owed` true, since both sides drop
/// by the same amount.
pub fn forfeit_reward(ctx: Context<ForfeitReward>, reward_index: u8) -> Result<()> {
    let index = reward_index as usize;
    require!(index < REWARD_NUM, ErrorCode::InvalidRewardIndex);

    let amount = ctx.accounts.personal_position.reward_infos[index].reward_amount_owed;
    if amount == 0 {
        return Ok(());
    }
    ctx.accounts
        .pool_state
        .load()?
        .check_unclaimed_reward(index, amount)?;

    ctx.accounts.personal_position.reward_infos[index].reward_amount_owed = 0;
    ctx.accounts
        .pool_state
        .load_mut()?
        .add_reward_claimed(index, amount)?;

    emit!(RewardForfeitedEvent {
        pool_state: ctx.accounts.pool_state.key(),
        position_nft_mint: ctx.accounts.personal_position.nft_mint,
        reward_index,
        amount,
    });
    Ok(())
}

/// Emitted when a position owner writes off an uncollectable reward.
#[event]
pub struct RewardForfeitedEvent {
    pub pool_state: Pubkey,
    pub position_nft_mint: Pubkey,
    pub reward_index: u8,
    pub amount: u64,
}
