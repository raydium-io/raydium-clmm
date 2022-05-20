use super::swap;
use super::SwapContext;
use crate::libraries::tick_math;
use crate::states::*;
use anchor_lang::prelude::*;
use std::collections::BTreeMap;

/// Performs a single exact input swap
pub fn exact_input_internal<'info>(
    accounts: &mut SwapContext<'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_in: u64,
    sqrt_price_limit_x32: u64,
) -> Result<u64> {
    let pool_state = AccountLoader::<PoolState>::try_from(&accounts.pool_state)?;
    let zero_for_one = accounts.input_vault.mint == pool_state.load()?.token_0;

    let balance_before = accounts.input_vault.amount;
    swap(
        Context::new(
            &crate::ID,
            accounts,
            remaining_accounts,
            BTreeMap::default(),
        ),
        i64::try_from(amount_in).unwrap(),
        if sqrt_price_limit_x32 == 0 {
            if zero_for_one {
                tick_math::MIN_SQRT_RATIO + 1
            } else {
                tick_math::MAX_SQRT_RATIO - 1
            }
        } else {
            sqrt_price_limit_x32
        },
    )?;

    accounts.input_vault.reload()?;
    Ok(accounts.input_vault.amount - balance_before)
}
