//! Self-describing layout for `remaining_accounts` on the v3 instruction set.
//!
//! Before v3 each instruction had its own positional convention whose segment boundaries the
//! client had to re-derive from pool state. Hook accounts have to sit in that tail, so v3 makes
//! the caller declare the segments: a list of `(type, length)` pairs consumed front to back.

use crate::error::ErrorCode;
use crate::states::pool::REWARD_NUM;
use anchor_lang::prelude::*;

/// One stretch of `remaining_accounts`: `length` accounts of kind `accounts_type`.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct RemainingAccountsSlice {
    pub accounts_type: AccountsType,
    /// Number of accounts in the stretch.
    pub length: u8,
}

/// What each stretch of `remaining_accounts` is, consumed front to back. Every account must be
/// covered, each type may appear once, and an instruction accepts only the types it reads.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RemainingAccountsInfo {
    pub slices: Vec<RemainingAccountsSlice>,
}

/// Kinds of segment a v3 instruction can be handed. Serialized as the variant index, so
/// variants may only be appended.
///
/// Hook legs are named after the instruction's own accounts: input/output for swaps and limit
/// orders, token_0 / token_1 for liquidity. A hook segment is the mint's hook program,
/// ExtraAccountMetaList PDA and extras, in any order.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccountsType {
    /// Tick arrays, in the order the instruction crosses them.
    TickArray,
    /// The pool's tick array bitmap extension; at most one.
    TickArrayBitmapExtension,
    /// Reward vault, funder token account, reward mint.
    RewardGroup,
    TransferHookInput,
    TransferHookOutput,
    TransferHookToken0,
    TransferHookToken1,
    TransferHookReward0,
    TransferHookReward1,
    TransferHookReward2,
    /// Looked up by key: `SupportMintAssociated` PDAs.
    MintAdmission,
}

/// Number of variants above; also the size of the duplicate-detection bitmap.
const ACCOUNTS_TYPE_COUNT: usize = 11;

/// The segments a v3 instruction found on the tape. Anything not declared is empty.
pub struct ParsedRemainingAccounts<'info> {
    pub tick_array: &'info [AccountInfo<'info>],
    pub bitmap_extension: &'info [AccountInfo<'info>],
    pub reward_group: &'info [AccountInfo<'info>],
    pub hook_input: &'info [AccountInfo<'info>],
    pub hook_output: &'info [AccountInfo<'info>],
    pub hook_token_0: &'info [AccountInfo<'info>],
    pub hook_token_1: &'info [AccountInfo<'info>],
    pub hook_reward: [&'info [AccountInfo<'info>]; REWARD_NUM],
    pub mint_admission: &'info [AccountInfo<'info>],
}

impl<'info> Default for ParsedRemainingAccounts<'info> {
    fn default() -> Self {
        Self {
            tick_array: &[],
            bitmap_extension: &[],
            reward_group: &[],
            hook_input: &[],
            hook_output: &[],
            hook_token_0: &[],
            hook_token_1: &[],
            hook_reward: [&[]; REWARD_NUM],
            mint_admission: &[],
        }
    }
}

impl<'info> ParsedRemainingAccounts<'info> {
    /// The single `TickArrayBitmapExtension` account, if the caller declared one.
    pub fn bitmap_extension(&self) -> Option<&'info AccountInfo<'info>> {
        self.bitmap_extension.first()
    }

    fn assign(&mut self, ty: AccountsType, seg: &'info [AccountInfo<'info>]) {
        match ty {
            AccountsType::TickArray => self.tick_array = seg,
            AccountsType::TickArrayBitmapExtension => self.bitmap_extension = seg,
            AccountsType::RewardGroup => self.reward_group = seg,
            AccountsType::TransferHookInput => self.hook_input = seg,
            AccountsType::TransferHookOutput => self.hook_output = seg,
            AccountsType::TransferHookToken0 => self.hook_token_0 = seg,
            AccountsType::TransferHookToken1 => self.hook_token_1 = seg,
            AccountsType::TransferHookReward0 => self.hook_reward[0] = seg,
            AccountsType::TransferHookReward1 => self.hook_reward[1] = seg,
            AccountsType::TransferHookReward2 => self.hook_reward[2] = seg,
            AccountsType::MintAdmission => self.mint_admission = seg,
        }
    }
}

/// Split `accounts` into the segments the tape declares.
///
/// The tape must account for every passed account exactly once: a type may not repeat and nothing may trail the last slice
/// (that is how stale clients silently pass reward accounts an instruction no longer reads).
/// Only the segment types in `allowed` may appear: one the instruction never reads would
/// otherwise be silently dropped.
pub fn parse_remaining_accounts<'info>(
    accounts: &'info [AccountInfo<'info>],
    info: &RemainingAccountsInfo,
    allowed: &[AccountsType],
) -> Result<ParsedRemainingAccounts<'info>> {
    let mut out = ParsedRemainingAccounts::default();
    let mut seen = [false; ACCOUNTS_TYPE_COUNT];
    let mut cursor = 0usize;

    for slice in info.slices.iter() {
        let ty = slice.accounts_type;
        require!(
            allowed.contains(&ty),
            ErrorCode::RemainingAccountsInfoMismatch
        );
        require!(!seen[ty as usize], ErrorCode::RemainingAccountsInfoMismatch);
        seen[ty as usize] = true;

        let end = cursor
            .checked_add(slice.length as usize)
            .ok_or(ErrorCode::RemainingAccountsInfoMismatch)?;
        require_gte!(
            accounts.len(),
            end,
            ErrorCode::RemainingAccountsInfoMismatch
        );

        let seg = &accounts[cursor..end];
        if ty == AccountsType::TickArrayBitmapExtension {
            require_gte!(1, seg.len(), ErrorCode::RemainingAccountsInfoMismatch);
        }
        out.assign(ty, seg);
        cursor = end;
    }

    require_eq!(
        cursor,
        accounts.len(),
        ErrorCode::RemainingAccountsInfoMismatch
    );
    Ok(out)
}

/// The hook segment that belongs to a given reward slot.
pub fn reward_hook_type(reward_index: u8) -> Result<AccountsType> {
    Ok(match reward_index {
        0 => AccountsType::TransferHookReward0,
        1 => AccountsType::TransferHookReward1,
        2 => AccountsType::TransferHookReward2,
        _ => return err!(ErrorCode::InvalidRewardIndex),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWAP: &[AccountsType] = &[
        AccountsType::TickArray,
        AccountsType::TickArrayBitmapExtension,
        AccountsType::TransferHookInput,
        AccountsType::TransferHookOutput,
    ];

    fn tape(pairs: &[(AccountsType, u8)]) -> RemainingAccountsInfo {
        RemainingAccountsInfo {
            slices: pairs
                .iter()
                .map(|(accounts_type, length)| RemainingAccountsSlice {
                    accounts_type: *accounts_type,
                    length: *length,
                })
                .collect(),
        }
    }

    #[test]
    fn empty_tape_accepts_only_empty_accounts() {
        assert!(parse_remaining_accounts(&[], &tape(&[]), SWAP).is_ok());
    }

    #[test]
    fn rejects_unknown_type() {
        // One slice: type, length.
        assert!(RemainingAccountsInfo::try_from_slice(&[1, 0, 0, 0, 10, 0]).is_ok());
        assert!(RemainingAccountsInfo::try_from_slice(&[1, 0, 0, 0, 11, 0]).is_err());
        assert!(RemainingAccountsInfo::try_from_slice(&[1, 0, 0, 0, 255, 0]).is_err());
    }

    #[test]
    fn rejects_duplicate_type() {
        assert!(parse_remaining_accounts(
            &[],
            &tape(&[(AccountsType::TickArray, 0), (AccountsType::TickArray, 0)]),
            SWAP
        )
        .is_err());
    }

    /// A hook leg is not capped: whatever the hook's list needs gets through.
    #[test]
    fn accepts_long_hook_leg() {
        let len = 9;
        let keys: Vec<Pubkey> = (0..len).map(|_| Pubkey::new_unique()).collect();
        let owner = Pubkey::new_unique();
        let mut lamports = vec![0u64; len];
        let mut data: Vec<Vec<u8>> = vec![vec![]; len];
        let accounts: Vec<AccountInfo> = keys
            .iter()
            .zip(lamports.iter_mut())
            .zip(data.iter_mut())
            .map(|((key, lamports), data)| {
                AccountInfo::new(key, false, false, lamports, data, &owner, false)
            })
            .collect();

        let hook_input = AccountsType::TransferHookInput;
        let parsed =
            parse_remaining_accounts(&accounts, &tape(&[(hook_input, len as u8)]), SWAP).unwrap();
        assert_eq!(parsed.hook_input.len(), len);
    }

    /// Even an empty segment of a type the instruction never reads is refused.
    #[test]
    fn rejects_type_not_allowed() {
        let reward_group = AccountsType::RewardGroup;
        assert!(parse_remaining_accounts(&[], &tape(&[(reward_group, 0)]), SWAP).is_err());
    }

    #[test]
    fn rejects_tape_longer_than_accounts() {
        assert!(
            parse_remaining_accounts(&[], &tape(&[(AccountsType::TickArray, 1)]), SWAP).is_err()
        );
    }

    /// The mirror of the case above, and the one that matters: a tape that under-declares
    /// leaves accounts nobody claimed. Silently ignoring them is how an account the caller
    /// smuggled in would reach a CPI it was never meant to.
    #[test]
    fn rejects_accounts_longer_than_tape() {
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let (mut lamports_a, mut lamports_b) = (0u64, 0u64);
        let (mut data_a, mut data_b): (Vec<u8>, Vec<u8>) = (vec![], vec![]);
        let a = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports_a,
            &mut data_a,
            &owner,
            false,
        );
        let b = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports_b,
            &mut data_b,
            &owner,
            false,
        );
        let accounts = [a, b];

        assert!(
            parse_remaining_accounts(&accounts, &tape(&[(AccountsType::TickArray, 2)]), SWAP)
                .is_ok()
        );
        // one account left undeclared
        assert!(
            parse_remaining_accounts(&accounts, &tape(&[(AccountsType::TickArray, 1)]), SWAP)
                .is_err()
        );
        // nothing declared at all
        assert!(parse_remaining_accounts(&accounts, &tape(&[]), SWAP).is_err());
    }

    /// `TickArrayBitmapExtension` is a single account, never a list.
    #[test]
    fn rejects_more_than_one_bitmap_extension() {
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let (mut lamports_a, mut lamports_b) = (0u64, 0u64);
        let (mut data_a, mut data_b): (Vec<u8>, Vec<u8>) = (vec![], vec![]);
        let a = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports_a,
            &mut data_a,
            &owner,
            false,
        );
        let b = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports_b,
            &mut data_b,
            &owner,
            false,
        );
        let accounts = [a, b];
        let bitmap = AccountsType::TickArrayBitmapExtension;

        assert!(parse_remaining_accounts(&accounts[..1], &tape(&[(bitmap, 1)]), SWAP).is_ok());
        assert!(parse_remaining_accounts(&accounts, &tape(&[(bitmap, 2)]), SWAP).is_err());
    }
}
