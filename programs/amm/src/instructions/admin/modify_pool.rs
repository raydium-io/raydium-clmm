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

pub fn modify_pool(ctx: Context<ModifyPool>, param: u8, val: Vec<u128>, index: i32) -> Result<()> {
    let mut pool_state = ctx.accounts.pool_state.load_mut()?;
    let match_param = Some(param);
    match match_param {
        Some(0) => {
            // update status
            require_gte!(255, val[0]);
            pool_state.set_status(val[0] as u8);
        }
        Some(1) => {
            // update token fee
            pool_state.liquidity = val[0];
        }
        Some(2) => {
            // update claimed token fee
            require_eq!(val.len(), 2);

            require_gt!(u64::max_value() as u128, val[0]);
            require_gt!(u64::max_value() as u128, val[1]);

            let new_total_fees_claimed_token_0 = val[0] as u64;
            let new_total_fees_claimed_token_1 = val[1] as u64;

            require_gte!(
                pool_state.total_fees_token_0,
                new_total_fees_claimed_token_0
            );
            require_gte!(
                pool_state.total_fees_token_1,
                new_total_fees_claimed_token_1
            );
            pool_state.total_fees_claimed_token_0 = new_total_fees_claimed_token_0;
            pool_state.total_fees_claimed_token_1 = new_total_fees_claimed_token_1;
        }
        Some(3) => {
            // update claimed reward
            require_gte!(REWARD_NUM, val.len());
            for i in 0..val.len() {
                require_gt!(u64::max_value() as u128, val[i]);

                let new_reward_claimed = val[i] as u64;

                require_gte!(
                    pool_state.reward_infos[i].reward_total_emissioned,
                    new_reward_claimed
                );
                pool_state.reward_infos[i].reward_claimed = new_reward_claimed;
            }
        }
        Some(4) => {
            let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
            let tick_array_info = remaining_accounts_iter.next().unwrap();
            let mut tick_array_current = TickArrayState::load_mut(tick_array_info)?;
            require_keys_eq!(tick_array_current.pool_id, ctx.accounts.pool_state.key());
            let tick_state = tick_array_current
                .get_tick_state_mut(index, pool_state.tick_spacing.into())
                .unwrap();
            tick_state.cross(
                pool_state.fee_growth_global_0_x64,
                pool_state.fee_growth_global_1_x64,
                &pool_state.reward_infos,
            );
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }

    Ok(())
}
