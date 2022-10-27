use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ModifyPool<'info> {
    /// Address to be set as operation account owner.
    #[account(
        mut,
        address = crate::admin::id()
    )]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,
}

pub fn modify_pool(ctx: Context<ModifyPool>, param: u8, val: u128) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let match_param = Some(param);
    match match_param {
        Some(0) => {
            require_gte!(255, val);
            pool_state.set_status(val as u8);
        }
        Some(1) => {
            pool_state.liquidity = val;
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }

    Ok(())
}
