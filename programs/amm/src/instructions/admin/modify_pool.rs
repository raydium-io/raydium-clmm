use crate::error::ErrorCode;
use crate::states::*;
use crate::util::transfer_from_pool_vault_to_user;
use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

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

pub fn modify_pool<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ModifyPool<'info>>,
    param: u8,
    val: Vec<u128>,
    index: i32,
) -> Result<()> {
    let match_param = Some(param);
    match match_param {
        Some(0) => {
            // update pool status
            require_gte!(255, val[0]);
            let mut pool_state = ctx.accounts.pool_state.load_mut()?;
            pool_state.set_status(val[0] as u8);
        }
        Some(1) => {
            // update pool liquidity
            let mut pool_state = ctx.accounts.pool_state.load_mut()?;
            pool_state.liquidity = val[0];
        }
        Some(2) => {
            // update pool total_fees_claimed_token_0 and  total_fees_claimed_token_1
            require_eq!(val.len(), 2);

            require_gt!(u64::max_value() as u128, val[0]);
            require_gt!(u64::max_value() as u128, val[1]);

            let new_total_fees_claimed_token_0 = val[0] as u64;
            let new_total_fees_claimed_token_1 = val[1] as u64;

            let mut pool_state = ctx.accounts.pool_state.load_mut()?;

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
                let mut pool_state = ctx.accounts.pool_state.load_mut()?;
                require_gte!(
                    pool_state.reward_infos[i].reward_total_emissioned,
                    new_reward_claimed
                );
                pool_state.reward_infos[i].reward_claimed = new_reward_claimed;
            }
        }
        Some(4) => {
            // update tick data ,cross tick
            let pool_state = ctx.accounts.pool_state.load_mut()?;
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
        Some(5) => {
            // update personal and protocol position fee_growth_inside
            let mut remaining_accounts_iter = ctx.remaining_accounts.iter();
            let personal_position_info = remaining_accounts_iter.next().unwrap();
            let protocol_position_info = remaining_accounts_iter.next().unwrap();
            let personal_position =
                &mut Account::<PersonalPositionState>::try_from(personal_position_info)?;
            let protocol_position =
                &mut Account::<ProtocolPositionState>::try_from(protocol_position_info)?;
            let fee_growth_inside_0_last_x64 = val[0];
            let fee_growth_inside_1_last_x64 = val[1];

            personal_position.fee_growth_inside_0_last_x64 = fee_growth_inside_0_last_x64;
            personal_position.fee_growth_inside_1_last_x64 = fee_growth_inside_1_last_x64;

            protocol_position.fee_growth_inside_0_last_x64 = fee_growth_inside_0_last_x64;
            protocol_position.fee_growth_inside_1_last_x64 = fee_growth_inside_1_last_x64;

            personal_position.exit(ctx.program_id)?;
            protocol_position.exit(ctx.program_id)?;
        }
        Some(6) => {
            // withdraw from pool vault
            let mut remaining_accounts_iter = ctx.remaining_accounts.iter();

            let from_vault = Account::<TokenAccount>::try_from(remaining_accounts_iter.next().unwrap())?;
            let recipient_token_account =
                Account::<TokenAccount>::try_from(remaining_accounts_iter.next().unwrap())?;
            let token_program =
                Program::<Token>::try_from(remaining_accounts_iter.next().unwrap())?;

            require_gt!(u64::max_value() as u128, val[0]);
            let amount = val[0] as u64;

            assert!(
                ctx.accounts.pool_state.load()?.token_vault_0 == from_vault.key()
                    || ctx.accounts.pool_state.load()?.token_vault_1 == from_vault.key()
            );
            assert!(from_vault.mint == recipient_token_account.mint);

            transfer_from_pool_vault_to_user(
                &ctx.accounts.pool_state,
                &from_vault,
                &recipient_token_account,
                &token_program,
                amount,
            )?;
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }

    Ok(())
}
