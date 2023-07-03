use crate::error::ErrorCode;
use crate::libraries::tick_math;
use crate::swap::swap_internal;
use crate::util::*;
use crate::{states::*, util};
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, Token2022, TokenAccount};
use std::collections::VecDeque;
#[derive(Accounts)]
pub struct SwapSingleV2<'info> {
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// SPL program 2022 for token transfers
    pub token_program_2022: Program<'info, Token2022>,

    /// The mint of token vault 0
    #[account(
        address = input_vault.mint
    )]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token vault 1
    #[account(
        address = output_vault.mint
    )]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,
    // remaining accounts
    // tick_array_account_1
    // tick_array_account_2
    // tick_array_account_...
}

/// Performs a single exact input/output swap
/// if is_base_input = true, return vaule is the max_amount_out, otherwise is min_amount_in
pub fn exact_internal<'info>(
    ctx: &mut SwapSingleV2<'info>,
    remaining_accounts: &[AccountInfo<'info>],
    amount_specified: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<u64> {
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;

    let amount_0;
    let amount_1;
    let zero_for_one;
    let swap_price_before;

    let input_balance_before = ctx.input_vault.amount;
    let output_balance_before = ctx.output_vault.amount;

    let mut transfer_fee = 0;
    if is_base_input {
        transfer_fee = util::get_transfer_fee(*ctx.input_vault_mint.clone(), amount_specified).unwrap();
    }

    {
        swap_price_before = ctx.pool_state.load()?.sqrt_price_x64;
        let pool_state = &mut ctx.pool_state.load_mut()?;
        zero_for_one = ctx.input_vault.mint == pool_state.token_mint_0;

        require_gt!(block_timestamp, pool_state.open_time);

        require!(
            if zero_for_one {
                ctx.input_vault.key() == pool_state.token_vault_0
                    && ctx.output_vault.key() == pool_state.token_vault_1
            } else {
                ctx.input_vault.key() == pool_state.token_vault_1
                    && ctx.output_vault.key() == pool_state.token_vault_0
            },
            ErrorCode::InvalidInputPoolVault
        );

        let tick_array_states = &mut VecDeque::new();
        for tick_array_info in remaining_accounts {
            tick_array_states.push_back(TickArrayState::load_mut(tick_array_info)?);
        }

        (amount_0, amount_1) = swap_internal(
            &ctx.amm_config,
            pool_state,
            tick_array_states,
            &mut ctx.observation_state.load_mut()?,
            amount_specified - transfer_fee,
            if sqrt_price_limit_x64 == 0 {
                if zero_for_one {
                    tick_math::MIN_SQRT_PRICE_X64 + 1
                } else {
                    tick_math::MAX_SQRT_PRICE_X64 - 1
                }
            } else {
                sqrt_price_limit_x64
            },
            zero_for_one,
            is_base_input,
            oracle::block_timestamp(),
        )?;

        #[cfg(feature = "enable-log")]
        msg!(
            "exact_swap_internal, is_base_input:{}, amount_0: {}, amount_1: {}",
            is_base_input,
            amount_0,
            amount_1
        );
        require!(
            amount_0 != 0 && amount_1 != 0,
            ErrorCode::TooSmallInputOrOutputAmount
        );
    }
    let (token_account_0, token_account_1, vault_0, vault_1, vault_0_mint, vault_1_mint) =
        if zero_for_one {
            (
                ctx.input_token_account.clone(),
                ctx.output_token_account.clone(),
                ctx.input_vault.clone(),
                ctx.output_vault.clone(),
                ctx.input_vault_mint.clone(),
                ctx.output_vault_mint.clone(),
            )
        } else {
            (
                ctx.output_token_account.clone(),
                ctx.input_token_account.clone(),
                ctx.output_vault.clone(),
                ctx.input_vault.clone(),
                ctx.output_vault_mint.clone(),
                ctx.input_vault_mint.clone(),
            )
        };

    if zero_for_one {
        if !is_base_input {
            transfer_fee = util::get_transfer_inverse_fee(*ctx.input_vault_mint.clone(), amount_0).unwrap();
        }
        //  x -> y, deposit x token from user to pool vault.
        transfer_from_user_to_pool_vault(
            &ctx.payer,
            &token_account_0,
            &vault_0,
            Some(*vault_0_mint),
            &ctx.token_program,
            Some(ctx.token_program_2022.to_account_info()),
            amount_0 + transfer_fee,
        )?;
        if vault_1.amount <= amount_1 {
            // freeze pool, disable all instructions
            ctx.pool_state.load_mut()?.set_status(255);
        }
        // x -> y，transfer y token from pool vault to user.
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_1,
            &token_account_1,
            Some(*vault_1_mint),
            &ctx.token_program,
            Some(ctx.token_program_2022.to_account_info()),
            amount_1,
        )?;
    } else {
        if !is_base_input {
            transfer_fee = util::get_transfer_inverse_fee(*ctx.input_vault_mint.clone(), amount_1).unwrap();
        }
        transfer_from_user_to_pool_vault(
            &ctx.payer,
            &token_account_1,
            &vault_1,
            Some(*vault_1_mint),
            &ctx.token_program,
            Some(ctx.token_program_2022.to_account_info()),
            amount_1 + transfer_fee,
        )?;
        if vault_0.amount <= amount_0 {
            // freeze pool, disable all instructions
            ctx.pool_state.load_mut()?.set_status(255);
        }
        transfer_from_pool_vault_to_user(
            &ctx.pool_state,
            &vault_0,
            &token_account_0,
            Some(*vault_0_mint),
            &ctx.token_program,
            Some(ctx.token_program_2022.to_account_info()),
            amount_0,
        )?;
    }
    ctx.output_vault.reload()?;
    ctx.input_vault.reload()?;

    let pool_state = ctx.pool_state.load()?;
    emit!(SwapEvent {
        pool_state: pool_state.key(),
        sender: ctx.payer.key(),
        token_account_0: token_account_0.key(),
        token_account_1: token_account_1.key(),
        amount_0,
        amount_1,
        zero_for_one,
        sqrt_price_x64: pool_state.sqrt_price_x64,
        liquidity: pool_state.liquidity,
        tick: pool_state.tick_current
    });
    if zero_for_one {
        require_gt!(swap_price_before, pool_state.sqrt_price_x64);
    } else {
        require_gt!(pool_state.sqrt_price_x64, swap_price_before);
    }

    if is_base_input {
        Ok(output_balance_before
            .checked_sub(ctx.output_vault.amount)
            .unwrap())
    } else {
        Ok(ctx
            .input_vault
            .amount
            .checked_sub(input_balance_before)
            .unwrap())
    }
}

pub fn swap_v2<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, SwapSingleV2<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let amount_result = exact_internal(
        ctx.accounts,
        ctx.remaining_accounts,
        amount,
        sqrt_price_limit_x64,
        is_base_input,
    )?;
    if is_base_input {
        require_gte!(
            amount_result,
            other_amount_threshold,
            ErrorCode::TooLittleOutputReceived
        );
    } else {
        require_gte!(
            other_amount_threshold,
            amount_result,
            ErrorCode::TooMuchInputPaid
        );
    }

    Ok(())
}

#[cfg(test)]
mod swap_test {

    use super::*;
    use crate::states::pool_test::build_pool;
    use crate::states::tick_array_test::{
        build_tick, build_tick_array_with_tick_states, TickArrayInfo,
    };
    use std::cell::RefCell;
    use std::cell::RefMut;
    use std::vec;

    pub fn get_tick_array_states_mut(
        deque_tick_array_states: &VecDeque<RefCell<TickArrayState>>,
    ) -> RefCell<VecDeque<RefMut<TickArrayState>>> {
        let mut tick_array_states = VecDeque::new();

        for tick_array_state in deque_tick_array_states {
            tick_array_states.push_back(tick_array_state.borrow_mut());
        }
        RefCell::new(tick_array_states)
    }

    fn build_swap_param<'info>(
        tick_current: i32,
        tick_spacing: u16,
        sqrt_price_x64: u128,
        liquidity: u128,
        tick_array_infos: Vec<TickArrayInfo>,
    ) -> (
        AmmConfig,
        RefCell<PoolState>,
        VecDeque<RefCell<TickArrayState>>,
        RefCell<ObservationState>,
    ) {
        let amm_config = AmmConfig {
            trade_fee_rate: 1000,
            tick_spacing,
            ..Default::default()
        };
        let pool_state = build_pool(tick_current, tick_spacing, sqrt_price_x64, liquidity);

        let observation_state = RefCell::new(ObservationState::default());
        observation_state.borrow_mut().pool_id = pool_state.borrow().key();

        let mut tick_array_states: VecDeque<RefCell<TickArrayState>> = VecDeque::new();
        for tick_array_info in tick_array_infos {
            tick_array_states.push_back(build_tick_array_with_tick_states(
                pool_state.borrow().key(),
                tick_array_info.start_tick_index,
                tick_spacing,
                tick_array_info.ticks,
            ));
            pool_state
                .borrow_mut()
                .flip_tick_array_bit(tick_array_info.start_tick_index)
                .unwrap();
        }

        (amm_config, pool_state, tick_array_states, observation_state)
    }

    #[cfg(test)]
    mod cross_tick_array_test {
        use super::*;

        #[test]
        fn zero_for_one_base_input_test() {
            let mut tick_current = -32395;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3651942632306380802;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32400), hasn't reached the next tick array initialized tick
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                12188240002,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 277065331032));
            assert!(amount_0 == 12188240002);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32400) in last step, now tickarray_current is the tickarray with start_index -36000,
            // so we pop the tickarray with start_index -32400
            // in this swap we will cross the tick(-32460), but not reach next tick (-32520)
            tick_array_states.pop_front();
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                121882400020,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32520
                    && pool_state.borrow().tick_current < -32460
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 536061033698));
            assert!(amount_0 == 121882400020);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -36000, cross the tick -32520
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                60941200010,
                3049500711113990606,
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32580
                    && pool_state.borrow().tick_current < -32520
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 790917615645));
            assert!(amount_0 == 60941200010);
        }

        #[test]
        fn zero_for_one_base_output_test() {
            let mut tick_current = -32395;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3651942632306380802;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32400), hasn't reached the next tick array initialized tick
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                477470480,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 277065331032));
            assert!(amount_1 == 477470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32400) in last step, now tickarray_current is the tickarray with start_index -36000,
            // so we pop the tickarray with start_index -32400
            // in this swap we will cross the tick(-32460), but not reach next tick (-32520)
            tick_array_states.pop_front();
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                4751002622,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32520
                    && pool_state.borrow().tick_current < -32460
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 536061033698));
            assert!(amount_1 == 4751002622);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -36000
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                2358130642,
                3049500711113990606,
                true,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -32580
                    && pool_state.borrow().tick_current < -32520
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 790917615645));
            assert!(amount_1 == 2358130642);
        }

        #[test]
        fn one_for_zero_base_input_test() {
            let mut tick_current = -32470;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3638127228312488926;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32460), hasn't reached the next tick array initialized tick
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                887470480,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 536061033698));
            assert!(amount_1 == 887470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32460) in last step, but not reached tick -32400, because -32400 is the next tickarray boundary,
            // so the tickarray_current still is the tick array with start_index -36000
            // in this swap we will cross the tick(-32400), but not reach next tick (-29220)
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                3087470480,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_1 == 3087470480);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -32400, cross the tick -29220
            tick_array_states.pop_front();
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                200941200010,
                5882283448660210779,
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 1330680689));
            assert!(amount_1 == 200941200010);
        }

        #[test]
        fn one_for_zero_base_output_test() {
            let mut tick_current = -32470;
            let mut liquidity = 5124165121219;
            let mut sqrt_price_x64 = 3638127228312488926;
            let (amm_config, pool_state, mut tick_array_states, observation_state) =
                build_swap_param(
                    tick_current,
                    60,
                    sqrt_price_x64,
                    liquidity,
                    vec![
                        TickArrayInfo {
                            start_tick_index: -36000,
                            ticks: vec![
                                build_tick(-32460, 1194569667438, 536061033698).take(),
                                build_tick(-32520, 790917615645, 790917615645).take(),
                                build_tick(-32580, 152146472301, 128451145459).take(),
                                build_tick(-32640, 2625605835354, -1492054447712).take(),
                            ],
                        },
                        TickArrayInfo {
                            start_tick_index: -32400,
                            ticks: vec![
                                build_tick(-32400, 277065331032, -277065331032).take(),
                                build_tick(-29220, 1330680689, -1330680689).take(),
                                build_tick(-28860, 6408486554, -6408486554).take(),
                            ],
                        },
                    ],
                );

            // just cross the tickarray boundary(-32460), hasn't reached the next tick array initialized tick
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                22796232052,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32460
                    && pool_state.borrow().tick_current < -32400
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 536061033698));
            assert!(amount_0 == 22796232052);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // cross the tickarray boundary(-32460) in last step, but not reached tick -32400, because -32400 is the next tickarray boundary,
            // so the tickarray_current still is the tick array with start_index -36000
            // in this swap we will cross the tick(-32400), but not reach next tick (-29220)
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                79023558189,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_0 == 79023558189);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;
            liquidity = pool_state.borrow().liquidity;

            // swap in tickarray with start_index -32400, cross the tick -29220
            tick_array_states.pop_front();
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                4315086194758,
                5882283448660210779,
                false,
                false,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 1330680689));
            assert!(amount_0 == 4315086194758);
        }
    }

    #[cfg(test)]
    mod find_next_initialized_tick_test {
        use super::*;

        #[test]
        fn zero_for_one_current_tick_array_not_initialized_test() {
            let tick_current = -28776;
            let liquidity = 624165121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![
                        build_tick(-32400, 277065331032, -277065331032).take(),
                        build_tick(-29220, 1330680689, -1330680689).take(),
                        build_tick(-28860, 6408486554, -6408486554).take(),
                    ],
                }],
            );

            // find the first initialzied tick(-28860) and cross it in tickarray
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                12188240002,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(
                pool_state.borrow().tick_current > -29220
                    && pool_state.borrow().tick_current < -28860
            );
            assert!(pool_state.borrow().sqrt_price_x64 < sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity + 6408486554));
            assert!(amount_0 == 12188240002);
        }

        #[test]
        fn one_for_zero_current_tick_array_not_initialized_test() {
            let tick_current = -32405;
            let liquidity = 1224165121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![
                        build_tick(-32400, 277065331032, -277065331032).take(),
                        build_tick(-29220, 1330680689, -1330680689).take(),
                        build_tick(-28860, 6408486554, -6408486554).take(),
                    ],
                }],
            );

            // find the first initialzied tick(-32400) and cross it in tickarray
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                12188240002,
                tick_math::get_sqrt_price_at_tick(-28860).unwrap(),
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(
                pool_state.borrow().tick_current > -32400
                    && pool_state.borrow().tick_current < -29220
            );
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(pool_state.borrow().liquidity == (liquidity - 277065331032));
            assert!(amount_1 == 12188240002);
        }
    }

    #[cfg(test)]
    mod liquidity_insufficient_test {
        use super::*;
        use crate::error::ErrorCode;
        #[test]
        fn no_enough_initialized_tickarray_in_pool_test() {
            let tick_current = -28776;
            let liquidity = 121219;
            let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![TickArrayInfo {
                    start_tick_index: -32400,
                    ticks: vec![build_tick(-28860, 6408486554, -6408486554).take()],
                }],
            );

            let result = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                12188240002,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            );
            assert!(result.is_err());
            assert_eq!(result.unwrap_err(), ErrorCode::LiquidityInsufficient.into());
        }
    }

    #[test]
    fn explain_why_zero_for_one_less_or_equal_current_tick() {
        let tick_current = -28859;
        let mut liquidity = 121219;
        let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
        let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
            tick_current,
            60,
            sqrt_price_x64,
            liquidity,
            vec![TickArrayInfo {
                start_tick_index: -32400,
                ticks: vec![
                    build_tick(-32400, 277065331032, -277065331032).take(),
                    build_tick(-29220, 1330680689, -1330680689).take(),
                    build_tick(-28860, 6408486554, -6408486554).take(),
                ],
            }],
        );

        // not cross tick(-28860), but pool.tick_current = -28860
        let (amount_0, amount_1) = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            25,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current < tick_current);
        assert!(pool_state.borrow().tick_current == -28860);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28860).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity);
        assert!(amount_0 == 25);

        // just cross tick(-28860), pool.tick_current = -28861
        let (amount_0, amount_1) = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            3,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current < tick_current);
        assert!(pool_state.borrow().tick_current == -28861);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity + 6408486554);
        assert!(amount_0 == 3);

        liquidity = pool_state.borrow().liquidity;

        // we swap just a little amount, let pool tick_current also equal -28861
        // but pool.sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861)
        let (amount_0, amount_1) = swap_internal(
            &amm_config,
            &mut pool_state.borrow_mut(),
            &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
            &mut observation_state.borrow_mut(),
            50,
            tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
            true,
            true,
            oracle::block_timestamp_mock() as u32,
        )
        .unwrap();
        println!("amount_0:{},amount_1:{}", amount_0, amount_1);
        assert!(pool_state.borrow().tick_current == -28861);
        assert!(
            pool_state.borrow().sqrt_price_x64 > tick_math::get_sqrt_price_at_tick(-28861).unwrap()
        );
        assert!(pool_state.borrow().liquidity == liquidity);
        assert!(amount_0 == 50);
    }

    #[cfg(test)]
    mod swap_edge_test {
        use super::*;

        #[test]
        fn zero_for_one_swap_edge_case() {
            let mut tick_current = -28859;
            let liquidity = 121219;
            let mut sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick_current).unwrap();
            let (amm_config, pool_state, tick_array_states, observation_state) = build_swap_param(
                tick_current,
                60,
                sqrt_price_x64,
                liquidity,
                vec![
                    TickArrayInfo {
                        start_tick_index: -32400,
                        ticks: vec![
                            build_tick(-32400, 277065331032, -277065331032).take(),
                            build_tick(-29220, 1330680689, -1330680689).take(),
                            build_tick(-28860, 6408486554, -6408486554).take(),
                        ],
                    },
                    TickArrayInfo {
                        start_tick_index: -28800,
                        ticks: vec![build_tick(-28800, 3726362727, -3726362727).take()],
                    },
                ],
            );

            // zero for one, just cross tick(-28860),  pool.tick_current = -28861 and pool.sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(-28860)
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                27,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current < tick_current);
            assert!(pool_state.borrow().tick_current == -28861);
            assert!(
                pool_state.borrow().sqrt_price_x64
                    == tick_math::get_sqrt_price_at_tick(-28860).unwrap()
            );
            assert!(pool_state.borrow().liquidity == liquidity + 6408486554);
            assert!(amount_0 == 27);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;

            // we swap just a little amount, it is completely taken by fees, the sqrt price and the tick will remain the same
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                1,
                tick_math::get_sqrt_price_at_tick(-32400).unwrap(),
                true,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current == tick_current);
            assert!(pool_state.borrow().tick_current == -28861);
            assert!(pool_state.borrow().sqrt_price_x64 == sqrt_price_x64);

            tick_current = pool_state.borrow().tick_current;
            sqrt_price_x64 = pool_state.borrow().sqrt_price_x64;

            // reverse swap direction, one_for_zero
            // Actually, the loop for this swap was executed twice because the previous swap happened to have `pool.tick_current` exactly on the boundary that is divisible by `tick_spacing`.
            // In the first iteration of this swap's loop, it found the initial tick (-28860), but at this point, both the initial and final prices were equal to the price at tick -28860.
            // This did not meet the conditions for swapping so both swap_amount_input and swap_amount_output were 0. The actual output was calculated in the second iteration of the loop.
            let (amount_0, amount_1) = swap_internal(
                &amm_config,
                &mut pool_state.borrow_mut(),
                &mut get_tick_array_states_mut(&tick_array_states).borrow_mut(),
                &mut observation_state.borrow_mut(),
                10,
                tick_math::get_sqrt_price_at_tick(-28800).unwrap(),
                false,
                true,
                oracle::block_timestamp_mock() as u32,
            )
            .unwrap();
            println!("amount_0:{},amount_1:{}", amount_0, amount_1);
            assert!(pool_state.borrow().tick_current > tick_current);
            assert!(pool_state.borrow().sqrt_price_x64 > sqrt_price_x64);
            assert!(
                pool_state.borrow().tick_current > -28860
                    && pool_state.borrow().tick_current <= -28800
            );
        }
    }
}
