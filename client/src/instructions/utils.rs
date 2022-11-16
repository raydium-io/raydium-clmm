use std::collections::VecDeque;

use anchor_lang::AccountDeserialize;
use anyhow::Result;
use raydium_amm_v3::libraries::fixed_point_64;
use raydium_amm_v3::libraries::*;
use raydium_amm_v3::states::*;
use solana_sdk::account::Account;
use std::ops::DerefMut;
use std::ops::Neg;

pub fn deserialize_anchor_account<T: AccountDeserialize>(account: &Account) -> Result<T> {
    let mut data: &[u8] = &account.data;
    T::try_deserialize(&mut data).map_err(Into::into)
}

pub const Q_RATIO: f64 = 1.0001;

pub fn tick_to_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick)
}

pub fn price_to_tick(price: f64) -> i32 {
    price.log(Q_RATIO) as i32
}

pub fn tick_to_sqrt_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick).sqrt()
}

pub fn tick_with_spacing(tick: i32, tick_spacing: i32) -> i32 {
    let mut compressed = tick / tick_spacing;
    if tick < 0 && tick % tick_spacing != 0 {
        compressed -= 1; // round towards negative infinity
    }
    compressed * tick_spacing
}

pub fn multipler(decimals: u8) -> f64 {
    (10_i32).checked_pow(decimals.try_into().unwrap()).unwrap() as f64
}

pub fn price_to_x64(price: f64) -> u128 {
    (price * fixed_point_64::Q64 as f64) as u128
}

pub fn from_x64_price(price: u128) -> f64 {
    price as f64 / fixed_point_64::Q64 as f64
}

pub fn price_to_sqrt_price_x64(price: f64, decimals_0: u8, decimals_1: u8) -> u128 {
    let price_with_decimals = price * multipler(decimals_1) / multipler(decimals_0);
    price_to_x64(price_with_decimals.sqrt())
}

pub fn sqrt_price_x64_to_price(price: u128, decimals_0: u8, decimals_1: u8) -> f64 {
    from_x64_price(price).powi(2) * multipler(decimals_0) / multipler(decimals_1)
}

// the top level state of the swap, the results of which are recorded in storage at the end
#[derive(Debug)]
pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    pub amount_specified_remaining: u64,
    // the amount already swapped out/in of the output/input asset
    pub amount_calculated: u64,
    // current sqrt(price)
    pub sqrt_price_x64: u128,
    // the tick associated with the current price
    pub tick: i32,
    // the current liquidity in range
    pub liquidity: u128,
}
#[derive(Default)]
struct StepComputations {
    // the price at the beginning of the step
    sqrt_price_start_x64: u128,
    // the next tick to swap to from the current tick in the swap direction
    tick_next: i32,
    // whether tick_next is initialized or not
    initialized: bool,
    // sqrt(price) for the next tick (1/0)
    sqrt_price_next_x64: u128,
    // how much is being swapped in in this step
    amount_in: u64,
    // how much is being swapped out
    amount_out: u64,
    // how much fee is being paid in
    fee_amount: u64,
}

pub fn get_out_put_amount_and_remaining_accounts(
    input_amount: u64,
    sqrt_price_limit_x64: Option<u128>,
    zero_for_one: bool,
    is_base_input: bool,
    pool_config: &AmmConfig,
    pool_state: &PoolState,
    tick_arrays: &mut VecDeque<TickArrayState>,
) -> Result<(u64, VecDeque<i32>), &'static str> {
    let (_, current_vaild_tick_array_start_index) =
        pool_state.get_first_initialized_tick_array(zero_for_one).unwrap();

    let (amount_calculated, tick_array_start_index_vec) = swap_compute(
        zero_for_one,
        is_base_input,
        pool_config.trade_fee_rate,
        input_amount,
        current_vaild_tick_array_start_index,
        sqrt_price_limit_x64.unwrap_or(0),
        pool_state,
        tick_arrays,
    )?;
    println!("tick_array_start_index:{:?}", tick_array_start_index_vec);

    Ok((amount_calculated, tick_array_start_index_vec))
}

fn swap_compute(
    zero_for_one: bool,
    is_base_input: bool,
    fee: u32,
    amount_specified: u64,
    current_vaild_tick_array_start_index: i32,
    sqrt_price_limit_x64: u128,
    pool_state: &PoolState,
    tick_arrays: &mut VecDeque<TickArrayState>,
) -> Result<(u64, VecDeque<i32>), &'static str> {
    if amount_specified == 0 {
        return Result::Err("amountSpecified must not be 0");
    }
    let sqrt_price_limit_x64 = if sqrt_price_limit_x64 == 0 {
        if zero_for_one {
            tick_math::MIN_SQRT_PRICE_X64 + 1
        } else {
            tick_math::MAX_SQRT_PRICE_X64 - 1
        }
    } else {
        sqrt_price_limit_x64
    };
    if zero_for_one {
        if sqrt_price_limit_x64 < tick_math::MIN_SQRT_PRICE_X64 {
            return Result::Err("sqrt_price_limit_x64 must greater than MIN_SQRT_PRICE_X64");
        }
        if sqrt_price_limit_x64 >= pool_state.sqrt_price_x64 {
            return Result::Err("sqrt_price_limit_x64 must smaller than current");
        }
    } else {
        if sqrt_price_limit_x64 > tick_math::MAX_SQRT_PRICE_X64 {
            return Result::Err("sqrt_price_limit_x64 must smaller than MAX_SQRT_PRICE_X64");
        }
        if sqrt_price_limit_x64 <= pool_state.sqrt_price_x64 {
            return Result::Err("sqrt_price_limit_x64 must greater than current");
        }
    }

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0,
        sqrt_price_x64: pool_state.sqrt_price_x64,
        tick: pool_state.tick_current,
        liquidity: pool_state.liquidity,
    };

    let mut tick_array_current = tick_arrays.pop_front().unwrap();
    if tick_array_current.start_tick_index != current_vaild_tick_array_start_index {
        return Result::Err("tick array start tick index does not match");
    }
    let mut tick_array_start_index_vec = VecDeque::new();
    tick_array_start_index_vec.push_back(tick_array_current.start_tick_index);
    let mut loop_count = 0;
    // loop across ticks until input liquidity is consumed, or the limit price is reached
    while state.amount_specified_remaining != 0
        && state.sqrt_price_x64 != sqrt_price_limit_x64
        && state.tick < tick_math::MAX_TICK
        && state.tick > tick_math::MIN_TICK
    {
        if loop_count > 10 {
            return Result::Err("loop_count limit");
        }
        let mut step = StepComputations::default();
        step.sqrt_price_start_x64 = state.sqrt_price_x64;
        // save the bitmap, and the tick account if it is initialized
        let mut next_initialized_tick = if let Some(tick_state) = tick_array_current
            .next_initialized_tick(state.tick, pool_state.tick_spacing, zero_for_one)
            .unwrap()
        {
            Box::new(*tick_state)
        } else {
            Box::new(TickState::default())
        };
        if !next_initialized_tick.is_initialized() {
            let current_vaild_tick_array_start_index =
                tick_array_bit_map::next_initialized_tick_array_start_index(
                    U1024(pool_state.tick_array_bitmap),
                    current_vaild_tick_array_start_index,
                    pool_state.tick_spacing.into(),
                    zero_for_one,
                )
                .unwrap();
            tick_array_current = tick_arrays.pop_front().unwrap();
            if tick_array_current.start_tick_index != current_vaild_tick_array_start_index {
                return Result::Err("tick array start tick index does not match");
            }
            tick_array_start_index_vec.push_back(tick_array_current.start_tick_index);
            let mut first_initialized_tick = tick_array_current
                .first_initialized_tick(zero_for_one)
                .unwrap();

            next_initialized_tick = Box::new(*first_initialized_tick.deref_mut());
        }
        step.tick_next = next_initialized_tick.tick;
        step.initialized = next_initialized_tick.is_initialized();
        if step.tick_next < MIN_TICK {
            step.tick_next = MIN_TICK;
        } else if step.tick_next > MAX_TICK {
            step.tick_next = MAX_TICK;
        }

        step.sqrt_price_next_x64 = tick_math::get_sqrt_price_at_tick(step.tick_next).unwrap();

        let target_price = if (zero_for_one && step.sqrt_price_next_x64 < sqrt_price_limit_x64)
            || (!zero_for_one && step.sqrt_price_next_x64 > sqrt_price_limit_x64)
        {
            sqrt_price_limit_x64
        } else {
            step.sqrt_price_next_x64
        };
        let swap_step = swap_math::compute_swap_step(
            state.sqrt_price_x64,
            target_price,
            state.liquidity,
            state.amount_specified_remaining,
            fee,
            is_base_input,
        );
        state.sqrt_price_x64 = swap_step.sqrt_price_next_x64;
        step.amount_in = swap_step.amount_in;
        step.amount_out = swap_step.amount_out;
        step.fee_amount = swap_step.fee_amount;

        if is_base_input {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .checked_sub(step.amount_in + step.fee_amount)
                .unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_add(step.amount_out)
                .unwrap();
        } else {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .checked_sub(step.amount_out)
                .unwrap();
            state.amount_calculated = state
                .amount_calculated
                .checked_add(step.amount_in + step.fee_amount)
                .unwrap();
        }

        if state.sqrt_price_x64 == step.sqrt_price_next_x64 {
            // if the tick is initialized, run the tick transition
            if step.initialized {
                let mut liquidity_net = next_initialized_tick.liquidity_net;
                if zero_for_one {
                    liquidity_net = liquidity_net.neg();
                }
                state.liquidity =
                    liquidity_math::add_delta(state.liquidity, liquidity_net).unwrap();
            }

            state.tick = if zero_for_one {
                step.tick_next - 1
            } else {
                step.tick_next
            };
        } else if state.sqrt_price_x64 != step.sqrt_price_start_x64 {
            // recompute unless we're on a lower tick boundary (i.e. already transitioned ticks), and haven't moved
            state.tick = tick_math::get_tick_at_sqrt_price(state.sqrt_price_x64).unwrap();
        }
        loop_count += 1;
    }

    Ok((state.amount_calculated, tick_array_start_index_vec))
}
