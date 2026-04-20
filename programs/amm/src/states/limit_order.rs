use crate::error::ErrorCode;
use crate::instructions::check_limit_order_amount;
use crate::libraries::fixed_point_64;
use crate::libraries::{big_num::U128, full_math::Upcast256};
use crate::states::tick_array::TickState;
use anchor_lang::prelude::*;

#[account]
#[derive(Default)]
pub struct LimitOrderState {
    pub pool_id: Pubkey,
    /// Owner of this limit order
    pub owner: Pubkey,
    pub tick_index: i32,
    pub zero_for_one: bool,
    /// Order phase of this limit order, aligned with `TickState.order_phase` for FIFO semantics
    pub order_phase: u64,
    /// Total amount of the limit order
    pub total_amount: u64,
    /// Filled amount of the limit order (informational, for events/display)
    pub filled_amount: u64,
    /// The unfilled amount when the current computation segment started.
    /// Set to total_amount on open, reset on decrease. Never updated during settle —
    /// this avoids cascading floor-division error across multiple settles.
    pub settle_base: u64,
    /// Cumulative output paid to user in the current segment. Reset on decrease.
    /// Each settle computes total_output from the segment base and pays the diff
    /// (total_output - settled_output), limiting dust deduction to once per segment.
    pub settled_output: u64,
    /// Order open time
    pub open_time: u64,
    /// Snapshot of `TickState.unfilled_ratio_x64` at the time this segment started (Q64.64).
    /// Initialized to Q64 on open; reset to tick's current ratio on decrease.
    pub unfilled_ratio_x64: u128,
    pub padding: [u64; 4],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DecreaseAmountResult {
    pub settled_output_amount: u64,
    pub real_decrease_amount: u64,
}

impl LimitOrderState {
    pub const LEN: usize = 8 + 32 + 32 + 4 + 1 + 8 + 8 + 8 + 8 + 8 + 8 + 16 + 8 * 4;

    /// Create a new limit order with FIFO queue mechanism
    pub fn initialize(
        &mut self,
        pool_id: Pubkey,
        owner: Pubkey,
        tick: i32,
        zero_for_one: bool,
        amount: u64,
        order_phase: u64,
        timestamp: u64,
    ) {
        self.pool_id = pool_id;
        self.owner = owner;
        self.tick_index = tick;
        self.zero_for_one = zero_for_one;
        self.order_phase = order_phase;
        self.total_amount = amount;
        self.filled_amount = 0;
        self.open_time = timestamp;
        self.unfilled_ratio_x64 = fixed_point_64::Q64;
        self.settle_base = amount;
        self.settled_output = 0;
        self.padding = [0; 4];
    }

    /// Check if order is fully filled
    pub fn is_fully_filled(&self) -> bool {
        self.total_amount == self.filled_amount
    }

    /// Get remaining unfilled amount
    pub fn get_unfilled_amount(&self) -> Result<u64> {
        self.total_amount
            .checked_sub(self.filled_amount)
            .ok_or(ErrorCode::CalculateOverflow.into())
    }

    /// Settle order using absolute computation with output diff.
    ///
    /// Instead of updating the ratio snapshot on each settle (which causes cascading
    /// floor-division error), the ratio and `remaining` (segment base) are kept fixed.
    /// Each settle computes the total output from the segment base and pays the diff
    /// against previously settled output. This limits rounding error to O(1) per segment
    /// instead of O(N) per settle.
    ///
    /// The `-1` dust deduction (for non-exact division) is applied once to the total
    /// effective filled, and the diff naturally distributes it — no per-settle flag needed.
    pub fn settle_filled_order(&mut self, tick_state: &TickState) -> Result<u64> {
        if self.settle_base == 0 {
            return Ok(0);
        }

        if self.order_phase == tick_state.order_phase {
            // Same phase, no fills
            return Ok(0);
        } else if self.order_phase.saturating_add(1) == tick_state.order_phase {
            // Part-filled: absolute computation from remaining (segment base).
            // Floor on ideal_remaining ensures each order's unfilled <= proportional share,
            // so the sum never exceeds tick.part_filled_orders_remaining.
            let numerator = U128::from(self.settle_base).as_u256()
                * U128::from(tick_state.unfilled_ratio_x64).as_u256();
            let denominator = U128::from(self.unfilled_ratio_x64).as_u256();

            let ideal_remaining = (numerator / denominator).as_u64();
            let is_exact = (numerator % denominator).is_zero();

            let total_filled = self.settle_base.saturating_sub(ideal_remaining);
            if total_filled == 0 {
                return Ok(0);
            }

            let effective_filled = if is_exact {
                total_filled
            } else {
                total_filled.saturating_sub(1)
            };

            let total_output = TickState::get_limit_order_output(
                effective_filled,
                tick_state.tick,
                self.zero_for_one,
            )?;

            let payout = total_output.saturating_sub(self.settled_output);

            // Always update filled_amount even if payout is 0 (e.g. filled=1,
            // effective=0 → output=0), so get_unfilled_amount() stays correct.
            self.filled_amount = self
                .total_amount
                .checked_sub(ideal_remaining)
                .ok_or(ErrorCode::CalculateOverflow)?;
            self.settled_output = total_output;

            Ok(payout)
        } else if self.order_phase.saturating_add(2) <= tick_state.order_phase {
            // Fully filled: all remaining consumed. Output diff recovers any dust
            // held back during partial-fill settles.
            let total_output = TickState::get_limit_order_output(
                self.settle_base,
                tick_state.tick,
                self.zero_for_one,
            )?;
            let payout = total_output.saturating_sub(self.settled_output);

            self.filled_amount = self.total_amount;
            self.settle_base = 0;
            self.settled_output = 0;

            Ok(payout)
        } else {
            return err!(ErrorCode::InvalidOrderPhase);
        }
    }

    pub fn increase_amount(&mut self, tick_state: &mut TickState, amount: u64) -> Result<()> {
        if self.order_phase != tick_state.order_phase {
            return err!(ErrorCode::InvalidOrderPhase);
        }
        // Same phase guarantees the order is in orders_amount, never matched.
        self.total_amount = self
            .total_amount
            .checked_add(amount)
            .ok_or(ErrorCode::CalculateOverflow)?;
        self.settle_base = self
            .settle_base
            .checked_add(amount)
            .ok_or(ErrorCode::CalculateOverflow)?;
        tick_state.orders_amount = tick_state
            .orders_amount
            .checked_add(amount)
            .ok_or(ErrorCode::CalculateOverflow)?;
        Ok(())
    }

    pub fn decrease_amount(
        &mut self,
        tick_state: &mut TickState,
        amount: u64,
    ) -> Result<DecreaseAmountResult> {
        // Step 1: Settle all filled first
        let settled_output_amount = self.settle_filled_order(tick_state)?;
        let unfilled_amount = self.get_unfilled_amount()?;
        if unfilled_amount == 0 {
            return Ok(DecreaseAmountResult {
                settled_output_amount,
                real_decrease_amount: 0,
            });
        }

        let real_decrease_amount = if self.order_phase == tick_state.order_phase {
            // Same phase: simple subtraction from orders_amount
            let decrease = amount.min(unfilled_amount);
            tick_state.orders_amount = tick_state
                .orders_amount
                .checked_sub(decrease)
                .ok_or(ErrorCode::CalculateOverflow)?;
            decrease
        } else if self.order_phase.saturating_add(1) == tick_state.order_phase {
            // Cap at part_filled_orders_remaining to prevent vault insolvency
            let decrease = amount
                .min(unfilled_amount)
                .min(tick_state.part_filled_orders_remaining);
            tick_state.part_filled_orders_remaining = tick_state
                .part_filled_orders_remaining
                .checked_sub(decrease)
                .ok_or(ErrorCode::CalculateOverflow)?;
            decrease
        } else {
            // Fully-filled orders are normalized to unfilled == 0 by the preceding
            // settle_filled_order and hit the early return above, impossible to trigger.
            return err!(ErrorCode::OrderAlreadyFilled);
        };

        if real_decrease_amount == 0 {
            return Ok(DecreaseAmountResult {
                settled_output_amount,
                real_decrease_amount: 0,
            });
        }

        self.total_amount = self
            .total_amount
            .checked_sub(real_decrease_amount)
            .ok_or(ErrorCode::CalculateOverflow)?;

        // Reset computation baseline for the new segment
        if self.order_phase.saturating_add(1) == tick_state.order_phase {
            let new_unfilled = self.get_unfilled_amount()?;
            self.settle_base = new_unfilled;
            self.unfilled_ratio_x64 = tick_state.unfilled_ratio_x64;
            self.settled_output = 0;
        } else {
            // Same phase: remaining = total_amount (all unfilled)
            self.settle_base = self.total_amount;
        }

        let remaining_amount = self.get_unfilled_amount()?;
        if remaining_amount > 0 {
            check_limit_order_amount(remaining_amount, self.tick_index, self.zero_for_one)?;
        }

        Ok(DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        })
    }
}

/// Emitted when a limit order is opened
#[event]
pub struct OpenLimitOrderEvent {
    /// The pool whose limit order was opened
    pub pool_id: Pubkey,
    /// The limit order account
    pub limit_order: Pubkey,
    /// Direction of the limit order (true if zero_for_one)
    pub zero_for_one: bool,
    /// Tick index of the limit order
    pub tick_index: i32,
    /// Total amount of the limit order
    pub total_amount: u64,
    /// Transfer fee of the limit order
    pub transfer_fee: u64,
}

#[event]
pub struct IncreaseLimitOrderEvent {
    /// The pool whose limit order was increased
    pub pool_id: Pubkey,
    /// The limit order account
    pub limit_order: Pubkey,
    /// Direction of the limit order (true if zero_for_one)
    pub zero_for_one: bool,
    /// Tick index of the limit order
    pub tick_index: i32,
    /// Total amount of the limit order
    pub total_amount: u64,
    /// Increased amount of the limit order
    pub increased_amount: u64,
    /// Transfer fee of the limit order
    pub transfer_fee: u64,
}

/// Emitted when a limit order is settled and proceeds are transferred
#[event]
pub struct SettleLimitOrderEvent {
    /// The pool whose limit order was settled
    pub pool_id: Pubkey,
    /// The limit order account
    pub limit_order: Pubkey,
    /// Direction of the limit order (true if zero_for_one)
    pub zero_for_one: bool,
    /// Tick index of the limit order
    pub tick_index: i32,
    /// Total amount of the limit order
    pub total_amount: u64,
    /// Filled amount of the limit order
    pub filled_amount: u64,
    /// Amount of output tokens transferred (excluding fees)
    pub settled_amount_out: u64,
}

#[event]
pub struct DecreaseLimitOrderEvent {
    /// The pool whose limit order was decreased
    pub pool_id: Pubkey,
    /// The limit order account
    pub limit_order: Pubkey,
    /// Direction of the limit order (true if zero_for_one)
    pub zero_for_one: bool,
    /// Tick index of the limit order
    pub tick_index: i32,
    /// Total amount of the limit order
    pub total_amount: u64,
    /// Filled amount of the limit order
    pub filled_amount: u64,
    /// Amount of output tokens settled (excluding fees)
    pub settled_output_amount: u64,
    /// Amount of input tokens returned to user
    pub decreased_amount: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::states::tick_array::TickState;

    /// Helper function to create a mock tick state
    fn create_mock_tick_state(
        tick: i32,
        order_phase: u64,
        limit_order_amount: u64,
        part_filled_orders_remaining: u64,
    ) -> TickState {
        TickState {
            tick,
            liquidity_net: 0,
            liquidity_gross: 1000000,
            fee_growth_outside_0_x64: 0,
            fee_growth_outside_1_x64: 0,
            reward_growths_outside_x64: [0; 3], // REWARD_NUM = 3
            order_phase,
            orders_amount: limit_order_amount,
            part_filled_orders_remaining,
            unfilled_ratio_x64: fixed_point_64::Q64,
            padding: [0; 3],
        }
    }

    fn open_order(
        amount: u64,
        zero_for_one: bool,
        order_phase: u64,
        tick_state: &mut TickState,
    ) -> LimitOrderState {
        let mut order = LimitOrderState::default();
        order.initialize(
            Pubkey::default(),
            Pubkey::default(),
            tick_state.tick,
            zero_for_one,
            amount,
            order_phase,
            0,
        );
        tick_state.orders_amount += amount;
        order
    }

    /// Helper: settle all orders and return total filled
    fn settle_all(orders: &mut [LimitOrderState], ts: &TickState) -> u64 {
        let mut total = 0u64;
        for o in orders.iter_mut() {
            total += o.settle_filled_order(ts).unwrap();
        }
        total
    }

    #[test]
    fn order_increase_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        // Increase order amount
        let result = order.increase_amount(&mut tick_state, 500);
        assert!(result.is_ok());
        assert_eq!(order.total_amount, 1500);

        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 1000);

        assert!(tick_state.order_phase == initial_order_phase + 1);

        // Try to increase after partially filled: not allowed (order_phase mismatch)
        let result = order.increase_amount(&mut tick_state, 500);
        assert!(result.is_err());
    }

    #[test]
    fn order_decrease_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Decrease order amount, not filled (same phase)
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 300).unwrap();
        assert_eq!(settled_output_amount, 0);
        assert_eq!(real_decrease_amount, 300);
        assert_eq!(order.total_amount, 700);

        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.part_filled_orders_remaining == 600);
        assert!(tick_state.order_phase == initial_order_phase + 1);

        // Decrease after partially filled:
        // settle first (100 filled), then decrease 300 from unfilled (600)
        // NEW: total_amount -= 300, filled_amount = 100 (accumulated)
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 300).unwrap();
        assert_eq!(settled_output_amount, 100);
        assert_eq!(real_decrease_amount, 300);
        assert_eq!(order.total_amount, 400);
        assert_eq!(order.filled_amount, 101);
        // part_remaining: 600-300=300, part_total: 700-300=400
        assert!(tick_state.part_filled_orders_remaining == 300);
        assert!(tick_state.orders_amount == 0);

        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // Only 300 in part_remaining, so all consumed
        assert!(tick_state.part_filled_orders_remaining == 0);

        // Decrease after all consumed: settle gives remaining 299
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 200).unwrap();
        assert_eq!(settled_output_amount, 299);
        assert_eq!(real_decrease_amount, 0);
        assert_eq!(order.total_amount, 400);
        assert_eq!(order.filled_amount, 400);
    }

    #[test]
    fn decrease_amount_exceeds_available_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 1500).unwrap();
        assert_eq!(settled_output_amount, 0);
        assert_eq!(real_decrease_amount, 1000);
        assert_eq!(order.total_amount, 0);
        assert_eq!(order.filled_amount, 0);
    }

    #[test]
    fn fully_filled_order_handling_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let output = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output, 1000);
        assert!(order.is_fully_filled());

        let result = order.increase_amount(&mut tick_state, 100);
        assert!(result.is_err());

        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 100).unwrap();
        assert_eq!(settled_output_amount, 0);
        assert_eq!(real_decrease_amount, 0);
    }

    #[test]
    fn multi_order_settlement_by_order_phase_test() {
        let tick = 0;
        let initial_order_phase = 1000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);
        let mut order1 = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        assert_eq!(order1.settle_filled_order(&mut tick_state).unwrap(), 0);

        let mut order2 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        assert_eq!(order2.settle_filled_order(&mut tick_state).unwrap(), 0);

        // Match 500 from 2000 orders_amount
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut order3 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Match 500 more from part_remaining
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        assert_eq!(order3.settle_filled_order(&mut tick_state).unwrap(), 0);

        // order1: index-based settle, 500 filled
        assert_eq!(order1.settle_filled_order(&mut tick_state).unwrap(), 500);
        assert_eq!(order2.settle_filled_order(&mut tick_state).unwrap(), 500);

        // Match 1000 more from part_remaining (exhausts it)
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        assert_eq!(order3.settle_filled_order(&mut tick_state).unwrap(), 0);

        // After previous settle, order1.total=500, order1.index synced
        // Now index=0 (all consumed), so new_remaining=0, filled=500
        assert_eq!(order1.settle_filled_order(&mut tick_state).unwrap(), 500);
        assert_eq!(order2.settle_filled_order(&mut tick_state).unwrap(), 500);

        // Match from orders_amount (order3)
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // order1/2: already total=0 after previous settle
        assert_eq!(order1.settle_filled_order(&mut tick_state).unwrap(), 0);
        assert_eq!(order2.settle_filled_order(&mut tick_state).unwrap(), 0);

        assert_eq!(order3.settle_filled_order(&mut tick_state).unwrap(), 500);
    }

    #[test]
    fn fifo_order_priority_preservation_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 300, 200);

        let result = tick_state
            .match_limit_order(150, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 150);
        // part_total decremented: 1000-150=850
        assert!(tick_state.part_filled_orders_remaining == 50);
        assert!(tick_state.orders_amount == 300);

        let result2 = tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result2.amount_out, 100);
        // 50 from part_remaining (part_total: 850-50=800), then 50 from orders_amount
        // overwritten: part_total=300, part_remaining=250
        assert!(tick_state.part_filled_orders_remaining == 250);
        assert!(tick_state.orders_amount == 0);
    }

    #[test]
    fn fifo_edge_case_empty_part_filled_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 500, 0);

        let result = tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 300);
        assert!(tick_state.part_filled_orders_remaining == 200);
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.order_phase == initial_order_phase + 1);
    }

    #[test]
    fn part_filled_orders_total_proportional_settlement_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        // Match 700 then settle
        tick_state
            .match_limit_order(700, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let output = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output, 700);
        // NEW: total stays 1000, filled accumulates to 701
        assert_eq!(order.total_amount, 1000);
        assert_eq!(order.filled_amount, 701);

        // Match remaining 300
        tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let output2 = order.settle_filled_order(&mut tick_state).unwrap();
        // Fully filled recovers the 1-token dust from the first partial settle
        assert_eq!(output2, 300);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn part_filled_orders_total_complete_consumption_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 200);

        let result = tick_state
            .match_limit_order(200, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 200);
        // part_total decremented: 1000-200=800
        assert!(tick_state.part_filled_orders_remaining == 0);
        assert!(tick_state.order_phase == initial_order_phase);
    }

    #[test]
    fn part_filled_orders_total_order_settlement_accuracy_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);
        let order_amounts = [100, 200, 300, 400]; // Total: 1000
        let mut orders = Vec::new();

        for amount in order_amounts {
            let order = open_order(
                amount,
                limit_zero_for_one,
                initial_order_phase,
                &mut tick_state,
            );
            orders.push(order);
        }

        // Match 800 of 1000: index = Q64 * 200/1000 = Q64/5
        tick_state
            .match_limit_order(800, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut total_filled = 0;
        for order in &mut orders {
            let output = order.settle_filled_order(&mut tick_state).unwrap();
            total_filled += output;
        }
        // Floor rounding on remaining can cause each order to claim up to 1 extra filled
        assert!(
            total_filled >= 800 && total_filled <= 800 + orders.len() as u64,
            "total_filled {} out of range",
            total_filled
        );
    }

    #[test]
    fn multi_user_orders_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut user_a = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let mut user_b = open_order(
            2000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Match 500 from 3000 → index = Q64 * 2500/3000
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut user_c = open_order(
            1500,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // A: floor(1000 * 2500/3000) = 833 remaining, filled=167, is_exact=false → output=166
        assert_eq!(user_a.settle_filled_order(&mut tick_state).unwrap(), 166);
        // B: floor(2000 * 2500/3000) = 1666 remaining, filled=334, is_exact=false → output=333
        assert_eq!(user_b.settle_filled_order(&mut tick_state).unwrap(), 333);
        // C: same phase, unfilled
        assert_eq!(user_c.settle_filled_order(&mut tick_state).unwrap(), 0);

        // Match 2500 more (exhausts part_remaining)
        tick_state
            .match_limit_order(2500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A/B: ratio=0 (all consumed), total_output = get_output(remaining).
        // Output diff recovers the 1-token dust from the first partial settle.
        assert_eq!(user_a.settle_filled_order(&mut tick_state).unwrap(), 834);
        assert_eq!(user_b.settle_filled_order(&mut tick_state).unwrap(), 1667);
        assert_eq!(user_c.settle_filled_order(&mut tick_state).unwrap(), 0);

        tick_state
            .match_limit_order(1, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(user_c.settle_filled_order(&mut tick_state).unwrap(), 1);

        tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // Exact division (ratio=0) → no dust, diff recovers the 1-token from first C settle
        assert_eq!(user_c.settle_filled_order(&mut tick_state).unwrap(), 1499);
    }

    #[test]
    fn mult_order_settlement_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut old_orders = Vec::new();
        for _i in 0..5 {
            old_orders.push(open_order(
                1000,
                limit_zero_for_one,
                tick_state.order_phase,
                &mut tick_state,
            ));
        }

        // Match 2000 from 5000
        tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut new_orders = Vec::new();
        for _i in 0..2 {
            new_orders.push(open_order(
                1000,
                limit_zero_for_one,
                tick_state.order_phase,
                &mut tick_state,
            ));
        }

        // Match 1000 from part_remaining: part_total also decremented
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 2000);
        assert!(tick_state.orders_amount == 2000);

        // Match 2000 more from part_remaining
        tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 0);
        assert!(tick_state.orders_amount == 2000);

        // Old orders: phase+2 <= tick.phase → fully filled
        for order in &mut old_orders {
            assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 1000);
        }

        // New orders: same phase as tick → unfilled
        for order in &mut new_orders {
            assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 0);
        }
    }

    #[test]
    fn match_limit_order_base_input_flow_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, 100, true)
            .unwrap();
        assert_eq!(res1.amount_in, 499);
        assert_eq!(res1.amount_out, 499);
        assert!(res1.amm_fee_amount == 1);
        assert!(tick_state.part_filled_orders_remaining == 501);

        let res2 = tick_state
            .match_limit_order(1000, !limit_zero_for_one, true, 100, true)
            .unwrap();
        assert_eq!(res2.amount_in, 501);
        assert_eq!(res2.amount_out, 501);
        assert!(tick_state.part_filled_orders_remaining == 0);

        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 1000);
        assert!(order.is_fully_filled());
        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 0);
    }

    #[test]
    fn partial_then_double_settle_before_second_match_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Match 500 → index = Q64 * 500/1000 = Q64/2
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 500);

        // First settle: filled=500, total stays 1000, filled_amount=500
        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 500);
        assert_eq!(order.total_amount, 1000);
        assert_eq!(order.filled_amount, 500);

        // Second settle before new match: no change
        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 0);

        // Match 100 more
        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 400);

        // Third settle: additional filled=101, is_exact=false → output=100
        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 100);
        assert_eq!(order.total_amount, 1000);

        // Match remaining
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 0);

        // Ratio=0 (all consumed), exact → output diff recovers dust from earlier partial settle
        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 400);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn match_limit_order_fee_from_output_test() {
        let tick = 0;
        let fee_rate = 100;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, fee_rate, false)
            .unwrap();
        assert_eq!(res1.amount_in, 500);
        assert_eq!(res1.amount_out, 499);
        assert_eq!(res1.amm_fee_amount, 1);
        assert!(tick_state.part_filled_orders_remaining == 500);

        let res2 = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();
        assert_eq!(res2.amount_out, 400);
        assert_eq!(res2.amm_fee_amount, 1);
        let gross = res2.amount_out + res2.amm_fee_amount;
        assert!(tick_state.part_filled_orders_remaining == 500 - gross);

        let res3 = tick_state
            .match_limit_order(1000, !limit_zero_for_one, true, fee_rate, false)
            .unwrap();
        assert_eq!(res3.amount_out, 98);
        assert_eq!(res3.amm_fee_amount, 1);
        assert_eq!(res3.amount_out + res3.amm_fee_amount, 99);
        assert!(tick_state.part_filled_orders_remaining == 0);

        assert_eq!(order.settle_filled_order(&mut tick_state).unwrap(), 1000);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn match_limit_order_is_base_input_false_fee_on_input_test() {
        let tick = 0;
        let fee_rate = 100;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        let res = tick_state
            .match_limit_order(500, !limit_zero_for_one, false, fee_rate, true)
            .unwrap();
        assert_eq!(res.amount_out, 500);
        assert!(res.amm_fee_amount > 0);
        assert!(tick_state.part_filled_orders_remaining == 500);
    }

    #[test]
    fn match_limit_order_consume_from_both_part_remaining_and_orders_amount_test() {
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 500, 200);

        let result = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 400);
        // 200 from part_remaining (part_total:1000-200=800), then 200 from orders_amount
        // overwritten: part_total=500, part_remaining=300
        assert!(tick_state.part_filled_orders_remaining == 300);
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.order_phase == initial_order_phase + 1);
    }

    #[test]
    fn match_limit_order_exact_output_exceeds_total_unfilled_test() {
        let tick = 0;
        let fee_rate = 100;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();

        let res = tick_state
            .match_limit_order(1500, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();
        assert!(res.amount_out <= 500);
        assert!(res.amount_out >= 498);
        assert!(res.amm_fee_amount > 0);
        assert!(tick_state.part_filled_orders_remaining == 0);
    }

    #[test]
    fn match_limit_order_exact_input_exceeds_total_unfilled_recalculation_test() {
        let tick = 0;
        let fee_rate = 100;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        tick_state
            .match_limit_order(500, !limit_zero_for_one, true, fee_rate, true)
            .unwrap();

        let res = tick_state
            .match_limit_order(2000, !limit_zero_for_one, true, fee_rate, true)
            .unwrap();
        assert!(res.amount_out <= 501);
        assert!(res.amount_out >= 499);
        assert!(res.amount_in < 2000);
        assert!(tick_state.part_filled_orders_remaining == 0);
    }

    #[test]
    fn match_limit_order_zero_fee_rate_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, 0, true)
            .unwrap();
        assert_eq!(res1.amm_fee_amount, 0);
        assert_eq!(res1.amount_in, 500);
        assert_eq!(res1.amount_out, 500);

        let res2 = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, 0, false)
            .unwrap();
        assert_eq!(res2.amm_fee_amount, 0);
        assert_eq!(res2.amount_out, 400);
    }

    #[test]
    fn match_limit_order_part_filled_total_overwrite_after_complete_consumption_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 1000000, 0, 0);

        let mut order1 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut order2 = open_order(
            500,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let result = tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 300);
        assert!(tick_state.part_filled_orders_remaining == 200);
        assert!(tick_state.orders_amount == 0);

        assert_eq!(order1.settle_filled_order(&mut tick_state).unwrap(), 1000);
        assert!(order1.is_fully_filled());

        assert_eq!(order2.settle_filled_order(&mut tick_state).unwrap(), 300);
        assert_eq!(1000 + 300, 1300);
    }

    // ==================== Index-based decrease coverage ====================

    #[test]
    fn partial_decrease_in_part_filled_cohort_test() {
        // With index scheme, decrease directly subtracts from part_remaining/part_total
        // without changing tick.index, so other orders are unaffected.
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order_a = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let mut order_b = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // 50% fill → index = Q64/2
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A decrease 250: settle first (500 filled), then decrease 250 from unfilled (500)
        // NEW: total = 1000 - 250 = 750, filled = 500
        let dec_a = order_a.decrease_amount(&mut tick_state, 250).unwrap();
        assert_eq!(dec_a.settled_output_amount, 500);
        assert_eq!(dec_a.real_decrease_amount, 250);
        assert_eq!(order_a.total_amount, 750);
        assert_eq!(order_a.filled_amount, 500);
        // part_remaining: 1000-250=750, part_total: 2000-250=1750
        assert!(tick_state.part_filled_orders_remaining == 750);

        // Match 600 from part_remaining
        tick_state
            .match_limit_order(600, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 150);

        // B: index-based settle. B.index=Q64, tick.index = Q64/2 * 150/750 ≈ Q64/10
        // B: new_remaining = floor(1000 * (Q64/10) / Q64) = 99, filled=901
        let out_b = order_b.settle_filled_order(&tick_state).unwrap();
        assert_eq!(out_b, 900);

        // A: total=750, filled=500, index updated to Q64/2
        // After 600 more matched: tick.index ≈ Q64/10
        // A: current_unfilled = floor(250 * (Q64/10) / (Q64/2)) = 49, additional = 201, is_exact=false → 200
        let out_a = order_a.settle_filled_order(&tick_state).unwrap();
        assert_eq!(out_a, 200);

        // is_exact=false reduces output by 1 per non-exact settle
        assert_eq!(500 + 900 + 200, 1600);
    }

    #[test]
    fn sequential_decreases_preserve_ratio_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order_a = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let mut order_b = open_order(
            2000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let mut order_c = open_order(
            500,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Total=3500, match 1750 → 50% fill, index=Q64/2
        tick_state
            .match_limit_order(1750, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A: settle 500, decrease all 500 unfilled
        let dec_a = order_a.decrease_amount(&mut tick_state, 500).unwrap();
        assert_eq!(dec_a.settled_output_amount, 500);
        assert_eq!(dec_a.real_decrease_amount, 500);
        assert!(tick_state.part_filled_orders_remaining == 1250);

        // B: settle 1000, decrease 500
        let dec_b = order_b.decrease_amount(&mut tick_state, 500).unwrap();
        assert_eq!(dec_b.settled_output_amount, 1000);
        assert_eq!(dec_b.real_decrease_amount, 500);
        assert!(tick_state.part_filled_orders_remaining == 750);

        // C: index unchanged by decreases → same result as independent settle
        // ceil(500 * 1/2) = 250 unfilled, filled=250
        let out_c = order_c.settle_filled_order(&tick_state).unwrap();
        assert_eq!(out_c, 250);
    }

    #[test]
    fn decrease_after_settle_same_cohort_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // 50% fill
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // First decrease: settle 500, decrease 200
        // NEW: total = 1000 - 200 = 800, filled = 500
        let dec1 = order.decrease_amount(&mut tick_state, 200).unwrap();
        assert_eq!(dec1.settled_output_amount, 500);
        assert_eq!(dec1.real_decrease_amount, 200);
        assert_eq!(order.total_amount, 800);
        assert_eq!(order.filled_amount, 500);

        // Second decrease: no new fill, decrease 100
        // NEW: total = 800 - 100 = 700, filled = 500
        let dec2 = order.decrease_amount(&mut tick_state, 100).unwrap();
        assert_eq!(dec2.settled_output_amount, 0);
        assert_eq!(dec2.real_decrease_amount, 100);
        assert_eq!(order.total_amount, 700);
    }

    #[test]
    fn decrease_in_same_phase_test() {
        // Decrease when order is still in same phase (not yet matched)
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        let dec = order.decrease_amount(&mut tick_state, 300).unwrap();
        assert_eq!(dec.settled_output_amount, 0);
        assert_eq!(dec.real_decrease_amount, 300);
        assert_eq!(order.total_amount, 700);
        assert!(tick_state.orders_amount == 700);

        // Can increase after decrease in same phase
        order.increase_amount(&mut tick_state, 200).unwrap();
        assert_eq!(order.total_amount, 900);
        assert!(tick_state.orders_amount == 900);
    }

    #[test]
    fn partial_decrease_fully_consumed_after_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // 50% fill
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Decrease 200: settle 500, total = 1000 - 200 = 800, filled = 500
        let dec = order.decrease_amount(&mut tick_state, 200).unwrap();
        assert_eq!(dec.settled_output_amount, 500);
        assert_eq!(dec.real_decrease_amount, 200);
        assert_eq!(order.total_amount, 800);

        // Match remaining from part_remaining (300 available)
        tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 0);

        let out = order.settle_filled_order(&tick_state).unwrap();
        assert_eq!(out, 300);
        assert!(order.is_fully_filled());

        // Conservation: settled + decreased + out = opened
        assert_eq!(
            dec.settled_output_amount + dec.real_decrease_amount + out,
            1000
        );
    }

    #[test]
    fn token_conservation_asymmetric_orders_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order_a = open_order(
            3000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let mut order_b = open_order(
            7000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // 40% fill
        tick_state
            .match_limit_order(4000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A: settle 1200 (is_exact=false, reduced by 1), decrease 1799 (capped by unfilled)
        // NEW: total = 3000 - 1799 = 1201, filled = 1201
        let dec_a = order_a.decrease_amount(&mut tick_state, 1800).unwrap();
        assert_eq!(dec_a.settled_output_amount, 1200);
        assert_eq!(dec_a.real_decrease_amount, 1799);
        assert_eq!(order_a.total_amount, 1201);
        assert!(order_a.is_fully_filled());

        // B: settle 2800 (is_exact=false, reduced by 1)
        let b_output = order_b.settle_filled_order(&tick_state).unwrap();
        assert_eq!(b_output, 2800);

        // is_exact=false reduces output by 1 per non-exact settle
        assert_eq!(1200 + 2800, 4000);
        // Conservation with dust: settled + decreased + unfilled <= opened
        let lhs = 1200u64 + 1799 + 2800 + order_b.get_unfilled_amount().unwrap();
        assert!(lhs <= 10000 && lhs >= 10000 - 2, "lhs={}", lhs);
    }

    #[test]
    fn full_lifecycle_partial_decrease_settle_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut tick_state = create_mock_tick_state(tick, 0, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Match 600
        tick_state
            .match_limit_order(600, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Decrease 200: settle 600 (is_exact=false, reduced by 1), total = 1000 - 200 = 800, filled = 601
        let dec1 = order.decrease_amount(&mut tick_state, 200).unwrap();
        assert_eq!(dec1.settled_output_amount, 600);
        assert_eq!(dec1.real_decrease_amount, 200);
        assert_eq!(order.total_amount, 800);

        // Match 100 from part_remaining
        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out = order.settle_filled_order(&tick_state).unwrap();
        assert_eq!(out, 99);
        assert_eq!(order.total_amount, 800);

        // Match remaining
        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out_final = order.settle_filled_order(&tick_state).unwrap();
        // Ratio=0 (all consumed), exact → dust recovered from earlier partial settle
        assert_eq!(out_final, 100);
        assert!(order.is_fully_filled());

        // Decrease on fully filled: no-op
        let dec2 = order.decrease_amount(&mut tick_state, 100).unwrap();
        assert_eq!(dec2.settled_output_amount, 0);
        assert_eq!(dec2.real_decrease_amount, 0);
    }

    // =========================================================================
    // Cancel-isolation tests
    // =========================================================================

    #[test]
    fn cancel_does_not_affect_other_order_settlement_test() {
        let tick = 0;
        let limit_zero_for_one = true;

        let out_b1 = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
                .unwrap();
            b.settle_filled_order(&ts).unwrap()
        };

        let out_b2 = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
                .unwrap();
            a.decrease_amount(&mut ts, u64::MAX).unwrap();
            b.settle_filled_order(&ts).unwrap()
        };

        assert_eq!(
            out_b1, out_b2,
            "A's decrease must not affect B's settlement"
        );
    }

    #[test]
    fn cancel_does_not_affect_other_order_settlement_non_round_test() {
        let tick = 0;
        let limit_zero_for_one = true;

        let (out_b1, out_c1) = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            open_order(37, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(83, limit_zero_for_one, 0, &mut ts);
            let mut c = open_order(131, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(127, !limit_zero_for_one, false, 0, true)
                .unwrap();
            (
                b.settle_filled_order(&ts).unwrap(),
                c.settle_filled_order(&ts).unwrap(),
            )
        };

        let (out_b2, out_c2) = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(37, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(83, limit_zero_for_one, 0, &mut ts);
            let mut c = open_order(131, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(127, !limit_zero_for_one, false, 0, true)
                .unwrap();
            a.decrease_amount(&mut ts, u64::MAX).unwrap();
            (
                b.settle_filled_order(&ts).unwrap(),
                c.settle_filled_order(&ts).unwrap(),
            )
        };

        // With index scheme, B and C get identical results
        assert_eq!(out_b1, out_b2);
        assert_eq!(out_c1, out_c2);

        // Solvency
        {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(37, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(83, limit_zero_for_one, 0, &mut ts);
            let mut c = open_order(131, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(127, !limit_zero_for_one, false, 0, true)
                .unwrap();
            let dec = a.decrease_amount(&mut ts, u64::MAX).unwrap();
            let out_b = b.settle_filled_order(&ts).unwrap();
            let out_c = c.settle_filled_order(&ts).unwrap();
            let total = dec.settled_output_amount + out_b + out_c;
            // Floor rounding on remaining can cause per-order fills to exceed proportional share
            assert!(total <= 127 + 3, "Solvency: {} > {}", total, 127 + 3);
        }
    }

    #[test]
    fn cancel_solvency_sequential_exits_non_round_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let consumed = 71u64;
        let amounts = [13u64, 29, 41, 17];
        let total_opened: u64 = amounts.iter().sum();

        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut orders: Vec<LimitOrderState> = amounts
            .iter()
            .map(|&a| open_order(a, limit_zero_for_one, 0, &mut ts))
            .collect();
        ts.match_limit_order(consumed, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let mut total_filled = 0u64;
        let mut total_decreased = 0u64;

        for order in orders.iter_mut() {
            let dec = order.decrease_amount(&mut ts, u64::MAX).unwrap();
            total_filled += dec.settled_output_amount;
            total_decreased += dec.real_decrease_amount;
        }

        // Floor rounding on remaining can cause per-order fills to slightly exceed consumed
        assert!(
            total_filled <= consumed + amounts.len() as u64,
            "Solvency: {} > {}",
            total_filled,
            consumed + amounts.len() as u64
        );
        // Decrease is capped at part_filled_orders_remaining; per-order conservation holds
        assert!(total_filled + total_decreased <= total_opened);
        let dust = total_opened - total_filled - total_decreased;
        assert!(dust <= amounts.len() as u64, "dust {} too large", dust);
    }

    #[test]
    fn interleaved_settle_cancel_settle_test() {
        let tick = 0;
        let limit_zero_for_one = true;

        let b_ref = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(800, !limit_zero_for_one, false, 0, true)
                .unwrap();
            b.settle_filled_order(&ts).unwrap()
        };

        let (b_first, b_second) = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(800, !limit_zero_for_one, false, 0, true)
                .unwrap();

            let first = b.settle_filled_order(&ts).unwrap();
            a.decrease_amount(&mut ts, u64::MAX).unwrap();
            // A's decrease doesn't change tick.index, so B gets 0 on second settle
            let second = b.settle_filled_order(&ts).unwrap();
            (first, second)
        };

        assert_eq!(b_first, b_ref);
        assert_eq!(b_second, 0);
    }

    #[test]
    fn partial_decrease_does_not_affect_other_order_settlement_test() {
        // A does a PARTIAL decrease (not full exit). B's settled amount must be identical.
        let tick = 0;
        let limit_zero_for_one = true;

        // Path 1: no decrease
        let out_b1 = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
                .unwrap();
            b.settle_filled_order(&ts).unwrap()
        };

        // Path 2: A partial decrease 200 (of 500 unfilled)
        let out_b2 = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
                .unwrap();
            a.decrease_amount(&mut ts, 200).unwrap();
            b.settle_filled_order(&ts).unwrap()
        };

        assert_eq!(
            out_b1, out_b2,
            "A's partial decrease must not affect B's settlement"
        );
    }

    #[test]
    fn decrease_then_match_solvency_test() {
        // After A decreases, a future match happens. Verify solvency.
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);

        // Match 50%
        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A settle + decrease 200
        let dec_a = a.decrease_amount(&mut ts, 200).unwrap();
        assert_eq!(dec_a.settled_output_amount, 500);
        assert_eq!(dec_a.real_decrease_amount, 200);

        // Future match: 400 from part_remaining
        ts.match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Settle both
        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();

        let total_filled = dec_a.settled_output_amount + out_a + out_b;
        let total_consumed = 1000 + 400;
        assert!(
            total_filled <= total_consumed,
            "Solvency: filled={} > consumed={}",
            total_filled,
            total_consumed
        );

        // Conservation: filled_output + decreased + remaining_unfilled = total_opened
        let a_unfilled = a.get_unfilled_amount().unwrap();
        let b_unfilled = b.get_unfilled_amount().unwrap();
        assert_eq!(
            total_filled + dec_a.real_decrease_amount + a_unfilled + b_unfilled,
            2000,
            "Conservation failed"
        );
    }

    #[test]
    fn interleaved_settle_decrease_match_settle_test() {
        // B settles → A decreases → match → B settles again.
        // B's cumulative settlement should be correct.
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);

        // Match 50%
        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // B settles first round
        let b_first = b.settle_filled_order(&ts).unwrap();
        assert_eq!(b_first, 500);

        // A decreases (settle + remove)
        a.decrease_amount(&mut ts, 200).unwrap();

        // New match: 400 from part_remaining=800
        ts.match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // B settles second round
        let b_second = b.settle_filled_order(&ts).unwrap();

        // B's share of the 400 match: B has 500 unfilled out of 800 total
        // B should get ~250 (500/800 * 400 = 250, with ceil rounding)
        assert!(b_second > 0, "B should get fills from the new match");

        // Solvency: total output tokens paid ≤ total consumed
        // (decrease returns input tokens, not output, so don't include it)
        let total_consumed = 1000 + 400;
        let a_settled_in_decrease = 500u64; // settled_output from A's decrease
        let a_settle2 = a.settle_filled_order(&ts).unwrap();
        let total_output = a_settled_in_decrease + a_settle2 + b_first + b_second;
        assert!(
            total_output <= total_consumed,
            "Solvency: output {} > consumed {}",
            total_output,
            total_consumed
        );

        // Conservation: output_filled + input_decreased + input_unfilled = total_opened
        let a_unfilled = a.get_unfilled_amount().unwrap();
        let b_unfilled = b.get_unfilled_amount().unwrap();
        let input_decreased = 200u64;
        assert_eq!(
            total_output + input_decreased + a_unfilled + b_unfilled,
            2000
        );
    }

    #[test]
    fn partial_decrease_non_round_isolation_test() {
        // Non-round amounts: A partial decrease, then B and C settle.
        // B and C results must be identical with or without A's decrease.
        let tick = 0;
        let limit_zero_for_one = true;
        let consumed = 127u64;

        // Path 1: no decrease
        let (b1, c1) = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            open_order(37, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(83, limit_zero_for_one, 0, &mut ts);
            let mut c = open_order(131, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(consumed, !limit_zero_for_one, false, 0, true)
                .unwrap();
            (
                b.settle_filled_order(&ts).unwrap(),
                c.settle_filled_order(&ts).unwrap(),
            )
        };

        // Path 2: A partial decrease (not full)
        let (b2, c2) = {
            let mut ts = create_mock_tick_state(tick, 0, 0, 0);
            let mut a = open_order(37, limit_zero_for_one, 0, &mut ts);
            let mut b = open_order(83, limit_zero_for_one, 0, &mut ts);
            let mut c = open_order(131, limit_zero_for_one, 0, &mut ts);
            ts.match_limit_order(consumed, !limit_zero_for_one, false, 0, true)
                .unwrap();
            // A partial decrease: only 10 of ~19 unfilled
            a.decrease_amount(&mut ts, 10).unwrap();
            (
                b.settle_filled_order(&ts).unwrap(),
                c.settle_filled_order(&ts).unwrap(),
            )
        };

        assert_eq!(b1, b2, "B must not change: {} vs {}", b1, b2);
        assert_eq!(c1, c2, "C must not change: {} vs {}", c1, c2);
    }

    // =========================================================================
    // Non-round number tests
    // =========================================================================

    #[test]
    fn non_round_ratio_preservation_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 100, 0, 0);
        let mut a = open_order(333, limit_zero_for_one, 100, &mut ts);
        let mut b = open_order(667, limit_zero_for_one, 100, &mut ts);

        ts.match_limit_order(413, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(ts.part_filled_orders_remaining == 587);

        // A: floor(333 * 587/1000) = 195 unfilled, filled=138, is_exact=false → output=137
        let dec = a.decrease_amount(&mut ts, u64::MAX).unwrap();
        assert_eq!(dec.settled_output_amount, 137);
        assert_eq!(dec.real_decrease_amount, 195);

        // B: floor(667 * 587/1000) = 391 unfilled, filled=276, is_exact=false → output=275
        let out_b = b.settle_filled_order(&ts).unwrap();
        assert_eq!(out_b, 275);

        // is_exact=false reduces output by 1 per non-exact settle
        assert_eq!(137 + 275, 412);
        let lhs = 137u64 + 195 + 275 + (667 - 276);
        assert!(lhs <= 1000 && lhs >= 1000 - 2, "lhs={}", lhs);
    }

    #[test]
    fn prime_amounts_sequential_exits_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 50, 0, 0);
        let mut a = open_order(37, limit_zero_for_one, 50, &mut ts);
        let mut b = open_order(83, limit_zero_for_one, 50, &mut ts);
        let mut c = open_order(131, limit_zero_for_one, 50, &mut ts);

        ts.match_limit_order(127, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A: floor(37 * 124/251) = 18 unfilled, filled=19, is_exact=false → output=18
        let dec_a = a.decrease_amount(&mut ts, u64::MAX).unwrap();
        assert_eq!(dec_a.settled_output_amount, 18);
        assert_eq!(dec_a.real_decrease_amount, 18);

        // B: floor(83 * 124/251) = 41 unfilled, filled=42, is_exact=false → output=41
        let dec_b = b.decrease_amount(&mut ts, u64::MAX).unwrap();
        assert_eq!(dec_b.settled_output_amount, 41);
        assert_eq!(dec_b.real_decrease_amount, 41);

        // C: floor(131 * 124/251) = 64 unfilled, filled=67, is_exact=false → output=66
        // Decrease capped at remaining part_filled_orders_remaining (124-18-41=65)
        let dec_c = c.decrease_amount(&mut ts, u64::MAX).unwrap();
        assert_eq!(dec_c.settled_output_amount, 66);
        assert_eq!(dec_c.real_decrease_amount, 64);

        let total_output = 18 + 41 + 66;
        let total_decreased = 18 + 41 + 64;
        // is_exact=false reduces output, solvency guaranteed
        assert!(
            total_output <= 127,
            "output {} > consumed 127",
            total_output
        );
        assert!(total_output + total_decreased <= 251);
    }

    #[test]
    fn small_amounts_ceil_partial_decrease_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 1, 0, 0);
        let mut a = open_order(3, limit_zero_for_one, 1, &mut ts);
        let mut b = open_order(7, limit_zero_for_one, 1, &mut ts);

        ts.match_limit_order(5, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A: floor(3 * 5/10) = floor(1.5) = 1 unfilled, filled=2, is_exact=false → output=1
        // Decrease 1: settled=1, decrease=1, total = 3-1 = 2, filled = 2
        let dec = a.decrease_amount(&mut ts, 1).unwrap();
        assert_eq!(dec.settled_output_amount, 1);
        assert_eq!(dec.real_decrease_amount, 1);
        assert_eq!(a.total_amount, 2);
        assert!(ts.part_filled_orders_remaining == 4);

        // B: floor(7 * 5/10) = floor(3.5) = 3 unfilled, filled=4, is_exact=false → output=3
        let out_b = b.settle_filled_order(&ts).unwrap();
        assert_eq!(out_b, 3);
    }

    #[test]
    fn extreme_asymmetry_no_underflow_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 1, 0, 0);
        let mut a = open_order(1, limit_zero_for_one, 1, &mut ts);
        let mut b = open_order(99999, limit_zero_for_one, 1, &mut ts);

        ts.match_limit_order(50000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A: floor(1 * 50000/100000) = floor(0.5) = 0 unfilled, filled=1, is_exact=false → output=0
        let dec = a.decrease_amount(&mut ts, u64::MAX).unwrap();
        assert_eq!(dec.settled_output_amount, 0);
        assert_eq!(dec.real_decrease_amount, 0);
        assert!(ts.part_filled_orders_remaining == 50000);

        // B: floor(99999 * 50000/100000) = floor(49999.5) = 49999 unfilled, filled=50000, is_exact=false → output=49999
        let out_b = b.settle_filled_order(&ts).unwrap();
        assert_eq!(out_b, 49999);
    }

    #[test]
    fn protocol_solvency_fills_never_exceed_consumed_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(2000, limit_zero_for_one, 0, &mut ts);
        let mut c = open_order(500, limit_zero_for_one, 0, &mut ts);

        ts.match_limit_order(1750, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let dec_a = a.decrease_amount(&mut ts, 200).unwrap();

        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let dec_b = b.decrease_amount(&mut ts, 500).unwrap();

        ts.match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();
        let out_c = c.settle_filled_order(&ts).unwrap();

        let total_settled =
            dec_a.settled_output_amount + dec_b.settled_output_amount + out_a + out_b + out_c;
        let total_consumed: u64 = 1750 + 1000 + 300;

        assert!(
            total_settled <= total_consumed,
            "Insolvency: {} > {}",
            total_settled,
            total_consumed
        );
    }

    #[test]
    fn token_conservation_comprehensive_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let a_opened = 3000u64;
        let b_opened = 7000u64;
        let mut a = open_order(a_opened, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(b_opened, limit_zero_for_one, 0, &mut ts);

        ts.match_limit_order(4000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let dec_a = a.decrease_amount(&mut ts, 500).unwrap();
        let dec_b = b.decrease_amount(&mut ts, 1000).unwrap();

        ts.match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();

        // Per-order conservation with dust: settled + decreased + unfilled <= opened
        // (is_exact=false reduces output by 1 per non-exact settle, dust stays in vault)
        let lhs_a = dec_a.settled_output_amount
            + dec_a.real_decrease_amount
            + out_a
            + a.get_unfilled_amount().unwrap();
        assert!(lhs_a <= a_opened && lhs_a >= a_opened - 2, "a: {}", lhs_a);
        let lhs_b = dec_b.settled_output_amount
            + dec_b.real_decrease_amount
            + out_b
            + b.get_unfilled_amount().unwrap();
        assert!(lhs_b <= b_opened && lhs_b >= b_opened - 2, "b: {}", lhs_b);
    }

    #[test]
    fn decrease_exact_unfilled_amount_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut order = open_order(1000, limit_zero_for_one, 0, &mut ts);

        ts.match_limit_order(600, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Decrease 400: settle 600 (is_exact=false), unfilled=399, decrease=min(400,399)=399
        // NEW: total = 1000 - 399 = 601, filled = 601
        let dec = order.decrease_amount(&mut ts, 400).unwrap();
        assert_eq!(dec.settled_output_amount, 600);
        assert_eq!(dec.real_decrease_amount, 399);
        assert_eq!(order.total_amount, 601);
        assert!(order.is_fully_filled());
        // 1 dust remains in part_remaining due to floor rounding
        assert!(ts.part_filled_orders_remaining == 1);
        assert!(ts.orders_amount == 0);
    }

    #[test]
    fn all_users_exit_cohort_then_settle_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut a = open_order(100, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(100, limit_zero_for_one, 0, &mut ts);
        let mut c = open_order(100, limit_zero_for_one, 0, &mut ts);

        ts.match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let dec_a = a.decrease_amount(&mut ts, u64::MAX).unwrap();
        let dec_b = b.decrease_amount(&mut ts, u64::MAX).unwrap();
        let dec_c = c.decrease_amount(&mut ts, u64::MAX).unwrap();

        let total_filled = a.filled_amount + b.filled_amount + c.filled_amount;
        let total_decreased =
            dec_a.real_decrease_amount + dec_b.real_decrease_amount + dec_c.real_decrease_amount;

        // Floor rounding: per-order fills can slightly exceed consumed
        assert!(total_filled <= 100 + 3, "filled {} too large", total_filled);
        // Decrease capped at part_filled_orders_remaining, dust may remain
        assert!(total_filled + total_decreased <= 300);
        let dust = 300 - total_filled - total_decreased;
        assert!(dust <= 3, "dust {} too large", dust);
    }

    #[test]
    fn settle_order_with_non_zero_tick_test() {
        let tick = 100;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut order = open_order(1000, limit_zero_for_one, 0, &mut ts);

        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let output = order.settle_filled_order(&ts).unwrap();
        assert!(output > 1000, "at tick>0, output should exceed input");
        assert!(order.is_fully_filled());
    }

    #[test]
    fn multiple_partial_fills_same_cohort_solvency_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut orders: Vec<LimitOrderState> = (0..5)
            .map(|_| open_order(1000, limit_zero_for_one, 0, &mut ts))
            .collect();

        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // Total consumed: 3000

        let total_settled = settle_all(&mut orders, &ts);
        // Floor rounding: each order can claim up to 1 extra per settle
        assert!(
            total_settled >= 3000 && total_settled <= 3000 + orders.len() as u64,
            "total_settled {} out of range",
            total_settled
        );

        ts.match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let total_settled_2 = settle_all(&mut orders, &ts);
        // is_exact=false reduces output; total_settled <= total_consumed
        assert!(
            total_settled + total_settled_2 <= 5000
                && total_settled + total_settled_2 >= 5000 - orders.len() as u64 * 2,
            "total {} out of range",
            total_settled + total_settled_2
        );
    }

    // =========================================================================
    // Multi-phase, multi-order, interleaved decrease tests
    // =========================================================================

    /// Cohort 1 fully consumed → cohort 2 partially matched with interleaved decreases.
    /// Verifies cross-cohort solvency and per-order conservation.
    #[test]
    fn cross_cohort_decrease_solvency_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);

        // Cohort 0: A(500), B(500)
        let mut a = open_order(500, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(500, limit_zero_for_one, 0, &mut ts);

        // Fully consume cohort 0
        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(ts.part_filled_orders_remaining == 0);

        // Cohort 1: C(800), D(1200)
        let mut c = open_order(800, limit_zero_for_one, ts.order_phase, &mut ts);
        let mut d = open_order(1200, limit_zero_for_one, ts.order_phase, &mut ts);

        // Partially match cohort 1: 1000 of 2000
        ts.match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A & B: phase+2 → fully filled
        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();
        assert_eq!(out_a, 500);
        assert_eq!(out_b, 500);
        assert!(a.is_fully_filled());
        assert!(b.is_fully_filled());

        // C decreases 200 (settle 400 first, then decrease 200 from 400 unfilled)
        let dec_c = c.decrease_amount(&mut ts, 200).unwrap();
        assert_eq!(dec_c.settled_output_amount, 400);
        assert_eq!(dec_c.real_decrease_amount, 200);

        // D settles: should be unaffected by C's decrease
        let out_d = d.settle_filled_order(&ts).unwrap();
        assert_eq!(out_d, 600);

        // Match more from part_remaining
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out_c2 = c.settle_filled_order(&ts).unwrap();
        let out_d2 = d.settle_filled_order(&ts).unwrap();

        // Solvency: total output ≤ total consumed
        let total_output = out_a + out_b + dec_c.settled_output_amount + out_d + out_c2 + out_d2;
        let total_consumed = 1000 + 1000 + 500;
        assert!(
            total_output <= total_consumed,
            "Insolvency: {} > {}",
            total_output,
            total_consumed
        );

        // Per-order conservation
        assert_eq!(
            dec_c.settled_output_amount
                + dec_c.real_decrease_amount
                + out_c2
                + c.get_unfilled_amount().unwrap(),
            800
        );
        assert_eq!(out_d + out_d2 + d.get_unfilled_amount().unwrap(), 1200);
    }

    /// Multiple match-decrease-match-decrease rounds within the same cohort.
    /// Ensures repeated interleaving doesn't corrupt ratio tracking.
    #[test]
    fn repeated_match_decrease_cycles_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);

        let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);
        let mut b = open_order(1000, limit_zero_for_one, 0, &mut ts);
        let mut c = open_order(1000, limit_zero_for_one, 0, &mut ts);

        let mut total_consumed = 0u64;
        let mut a_total_settled = 0u64;
        let mut a_total_decreased = 0u64;

        // Round 1: match 600, A decreases 100
        ts.match_limit_order(600, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 600;
        let dec = a.decrease_amount(&mut ts, 100).unwrap();
        a_total_settled += dec.settled_output_amount;
        a_total_decreased += dec.real_decrease_amount;

        // Round 2: match 400, B decreases 150
        ts.match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 400;
        let dec_b1 = b.decrease_amount(&mut ts, 150).unwrap();

        // Round 3: match 300, A decreases 50 more
        ts.match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 300;
        let dec2 = a.decrease_amount(&mut ts, 50).unwrap();
        a_total_settled += dec2.settled_output_amount;
        a_total_decreased += dec2.real_decrease_amount;

        // Round 4: match 500, C decreases 200
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 500;
        let dec_c1 = c.decrease_amount(&mut ts, 200).unwrap();

        // Final settle
        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();
        let out_c = c.settle_filled_order(&ts).unwrap();

        // Solvency (with floor-rounding dust tolerance: up to 1 per order per settle)
        let total_output = a_total_settled
            + out_a
            + dec_b1.settled_output_amount
            + out_b
            + dec_c1.settled_output_amount
            + out_c;
        assert!(
            total_output <= total_consumed + 10,
            "Insolvency: {} > {}",
            total_output,
            total_consumed + 10
        );

        // Per-order conservation with dust (is_exact=false reduces output by 1 per non-exact settle)
        let lhs_a = a_total_settled + a_total_decreased + out_a + a.get_unfilled_amount().unwrap();
        assert!(lhs_a <= 1000 && lhs_a >= 1000 - 3, "a: {}", lhs_a);
        let lhs_b = dec_b1.settled_output_amount
            + dec_b1.real_decrease_amount
            + out_b
            + b.get_unfilled_amount().unwrap();
        assert!(lhs_b <= 1000 && lhs_b >= 1000 - 3, "b: {}", lhs_b);
        let lhs_c = dec_c1.settled_output_amount
            + dec_c1.real_decrease_amount
            + out_c
            + c.get_unfilled_amount().unwrap();
        assert!(lhs_c <= 1000 && lhs_c >= 1000 - 3, "c: {}", lhs_c);
    }

    /// After partial decrease, new orders join the same tick.
    /// Verifies new cohort orders don't interfere with old cohort settlement.
    #[test]
    fn decrease_then_new_orders_join_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);

        let mut a = open_order(1000, limit_zero_for_one, 0, &mut ts);

        // Match 500 → cohort 0 now part-filled
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // A decreases 200
        let dec_a = a.decrease_amount(&mut ts, 200).unwrap();
        assert_eq!(dec_a.settled_output_amount, 500);
        assert_eq!(dec_a.real_decrease_amount, 200);

        // New orders join (cohort 1, in orders_amount)
        let mut b = open_order(600, limit_zero_for_one, ts.order_phase, &mut ts);
        let mut c = open_order(400, limit_zero_for_one, ts.order_phase, &mut ts);

        // Match 800: first exhausts part_remaining(300), then 500 from orders_amount(1000)
        ts.match_limit_order(800, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let out_a = a.settle_filled_order(&ts).unwrap();
        assert_eq!(out_a, 300); // A's remaining 300 fully consumed
        assert!(a.is_fully_filled());

        let out_b = b.settle_filled_order(&ts).unwrap();
        let out_c = c.settle_filled_order(&ts).unwrap();

        // Solvency
        let total_output = dec_a.settled_output_amount + out_a + out_b + out_c;
        assert!(
            total_output <= 500 + 800,
            "Insolvency: {} > {}",
            total_output,
            1300
        );

        // B and C conservation
        assert_eq!(out_b + b.get_unfilled_amount().unwrap(), 600);
        assert_eq!(out_c + c.get_unfilled_amount().unwrap(), 400);
    }

    /// Three full cohort transitions with interleaved decreases across cohorts.
    #[test]
    fn three_cohort_interleaved_decrease_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let mut total_consumed = 0u64;

        // Cohort 0: A(300)
        let mut a = open_order(300, limit_zero_for_one, ts.order_phase, &mut ts);

        // Partial match cohort 0: 100 of 300
        ts.match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 100;

        // A decreases 50 (settle ~100, decrease 50 from ~200 unfilled)
        let dec_a = a.decrease_amount(&mut ts, 50).unwrap();

        // Consume rest of part_remaining
        let remaining = ts.part_filled_orders_remaining;
        ts.match_limit_order(remaining, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += remaining;

        // Cohort 1: B(500), C(500)
        let mut b = open_order(500, limit_zero_for_one, ts.order_phase, &mut ts);
        let mut c = open_order(500, limit_zero_for_one, ts.order_phase, &mut ts);

        // Partial match cohort 1: 400 of 1000
        ts.match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 400;

        // B decreases 100
        let dec_b = b.decrease_amount(&mut ts, 100).unwrap();

        // Consume rest of cohort 1
        let remaining1 = ts.part_filled_orders_remaining;
        ts.match_limit_order(remaining1, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += remaining1;

        // Cohort 2: D(200)
        let mut d = open_order(200, limit_zero_for_one, ts.order_phase, &mut ts);

        // Match 100 from cohort 2
        ts.match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 100;

        // Final settles
        let out_a = a.settle_filled_order(&ts).unwrap();
        let out_b = b.settle_filled_order(&ts).unwrap();
        let out_c = c.settle_filled_order(&ts).unwrap();
        let out_d = d.settle_filled_order(&ts).unwrap();

        // A: fully consumed (phase + 2 <= current)
        assert!(a.is_fully_filled());

        // Solvency
        let total_output = dec_a.settled_output_amount
            + out_a
            + dec_b.settled_output_amount
            + out_b
            + out_c
            + out_d;
        assert!(
            total_output <= total_consumed,
            "Insolvency: {} > {}",
            total_output,
            total_consumed
        );

        // Per-order conservation with dust (is_exact=false reduces output by 1 per non-exact settle)
        let lhs_a = dec_a.settled_output_amount
            + dec_a.real_decrease_amount
            + out_a
            + a.get_unfilled_amount().unwrap();
        assert!(lhs_a <= 300 && lhs_a >= 300 - 2, "a: {}", lhs_a);
        let lhs_b = dec_b.settled_output_amount
            + dec_b.real_decrease_amount
            + out_b
            + b.get_unfilled_amount().unwrap();
        assert!(lhs_b <= 500 && lhs_b >= 500 - 2, "b: {}", lhs_b);
        let lhs_c = out_c + c.get_unfilled_amount().unwrap();
        assert!(lhs_c <= 500 && lhs_c >= 500 - 2, "c: {}", lhs_c);
        let lhs_d = out_d + d.get_unfilled_amount().unwrap();
        assert!(lhs_d <= 200 && lhs_d >= 200 - 2, "d: {}", lhs_d);
    }

    /// Same cohort: many orders, interleaved partial decreases between multiple matches.
    /// Stress test for ratio accumulation correctness.
    #[test]
    fn many_orders_interleaved_decrease_stress_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        let amounts = [137, 251, 89, 373, 199];
        let total_opened: u64 = amounts.iter().sum(); // 1049

        let mut orders: Vec<LimitOrderState> = amounts
            .iter()
            .map(|&a| open_order(a, limit_zero_for_one, 0, &mut ts))
            .collect();

        let mut total_consumed = 0u64;
        let mut settled_outputs = vec![0u64; 5];
        let mut decreased_inputs = vec![0u64; 5];

        // Match 300
        ts.match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 300;

        // Order 0 decreases 50
        let dec = orders[0].decrease_amount(&mut ts, 50).unwrap();
        settled_outputs[0] += dec.settled_output_amount;
        decreased_inputs[0] += dec.real_decrease_amount;

        // Match 200
        ts.match_limit_order(200, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 200;

        // Order 2 decreases 30
        let dec = orders[2].decrease_amount(&mut ts, 30).unwrap();
        settled_outputs[2] += dec.settled_output_amount;
        decreased_inputs[2] += dec.real_decrease_amount;

        // Order 4 decreases 100
        let dec = orders[4].decrease_amount(&mut ts, 100).unwrap();
        settled_outputs[4] += dec.settled_output_amount;
        decreased_inputs[4] += dec.real_decrease_amount;

        // Match 150
        ts.match_limit_order(150, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 150;

        // Order 1 decreases 80
        let dec = orders[1].decrease_amount(&mut ts, 80).unwrap();
        settled_outputs[1] += dec.settled_output_amount;
        decreased_inputs[1] += dec.real_decrease_amount;

        // Order 3 decreases 50
        let dec = orders[3].decrease_amount(&mut ts, 50).unwrap();
        settled_outputs[3] += dec.settled_output_amount;
        decreased_inputs[3] += dec.real_decrease_amount;

        // Match 100
        ts.match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        total_consumed += 100;

        // Final settles
        for (i, order) in orders.iter_mut().enumerate() {
            settled_outputs[i] += order.settle_filled_order(&ts).unwrap();
        }

        // Solvency: total settled output ≤ total consumed
        let total_settled: u64 = settled_outputs.iter().sum();
        assert!(
            total_settled <= total_consumed,
            "Insolvency: {} > {}",
            total_settled,
            total_consumed
        );

        // Per-order conservation with dust (is_exact=false reduces output by 1 per non-exact settle)
        for (i, order) in orders.iter().enumerate() {
            let unfilled = order.get_unfilled_amount().unwrap();
            let lhs = settled_outputs[i] + decreased_inputs[i] + unfilled;
            assert!(
                lhs <= amounts[i] as u64 && lhs >= amounts[i] as u64 - 3,
                "Conservation failed for order {}: {} vs {}",
                i,
                lhs,
                amounts[i]
            );
        }

        // Global conservation with dust
        let total_decreased: u64 = decreased_inputs.iter().sum();
        let total_unfilled: u64 = orders
            .iter()
            .map(|o| o.get_unfilled_amount().unwrap())
            .sum();
        let global_lhs = total_settled + total_decreased + total_unfilled;
        assert!(
            global_lhs <= total_opened && global_lhs >= total_opened - orders.len() as u64 * 3,
            "Global conservation: {} vs {}",
            global_lhs,
            total_opened
        );
    }

    /// part_filled_orders_remaining exactly consumed → new orders join → swap → settle all
    #[test]
    fn exhaust_part_remaining_then_new_orders_settle_test() {
        let tick = 0;
        let limit_zero_for_one = true;
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);

        // phase 0: old_a(600), old_b(400)
        let mut old_a = open_order(600, limit_zero_for_one, 0, &mut ts);
        let mut old_b = open_order(400, limit_zero_for_one, 0, &mut ts);

        // Partial match 500 of 1000 → phase +1, part_remaining=500, ratio=Q64*500/1000
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(ts.part_filled_orders_remaining == 500);
        assert!(ts.orders_amount == 0);
        let phase_after_first_match = ts.order_phase;

        // Exactly consume the remaining 500 → part_remaining=0, ratio=0, phase unchanged
        ts.match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(ts.part_filled_orders_remaining == 0);
        assert!(ts.order_phase == phase_after_first_match); // phase did NOT change
        assert!(ts.unfilled_ratio_x64 == 0); // ratio collapsed to 0

        // New orders join at current phase (same phase as tick)
        let mut new_c = open_order(800, limit_zero_for_one, ts.order_phase, &mut ts);
        let mut new_d = open_order(200, limit_zero_for_one, ts.order_phase, &mut ts);
        assert!(ts.orders_amount == 1000);

        // Swap consumes 600 from new orders → phase +1
        ts.match_limit_order(600, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Old orders: phase+2 <= tick.phase → fully filled
        let out_old_a = old_a.settle_filled_order(&ts).unwrap();
        let out_old_b = old_b.settle_filled_order(&ts).unwrap();
        assert_eq!(out_old_a, 600);
        assert_eq!(out_old_b, 400);
        assert!(old_a.is_fully_filled());
        assert!(old_b.is_fully_filled());

        // New orders: phase+1 == tick.phase → proportional fill (floor rounding)
        let out_new_c = new_c.settle_filled_order(&ts).unwrap();
        let out_new_d = new_d.settle_filled_order(&ts).unwrap();
        assert_eq!(out_new_c, 480); // is_exact=false → output reduced by 1
        assert_eq!(out_new_d, 120); // is_exact=false → output reduced by 1
        assert_eq!(out_new_c + out_new_d, 600);

        // is_exact=false ensures solvency: total output <= total consumed
        let total_output = out_old_a + out_old_b + out_new_c + out_new_d;
        assert_eq!(total_output, 1600);
    }

    #[test]
    fn match_tiny_amount_extreme_tick_zero_output_test() {
        // At MIN_TICK + 1, swap_direction=true: output = amount * tiny_price / Q64 → floors to 0
        let tick = -443635;
        let out = TickState::get_limit_order_output(1, tick, true).unwrap();
        assert_eq!(out, 0);

        // Limit order zfo=false, swap direction=true
        let mut ts = create_mock_tick_state(tick, 0, 0, 0);
        open_order(1, false, 0, &mut ts);

        // base_input=true: amount_in=1, amount_out=0
        // returning Ok with amount_out=0 and no orders consumed.
        let result = ts.match_limit_order(1, true, true, 0, true).unwrap();
        assert_eq!(result.amount_out, 0);
        assert_eq!(result.amount_in, 1);
        // tick state unchanged — nothing was actually consumed
        assert!(ts.orders_amount == 1);
        assert!(ts.part_filled_orders_remaining == 0);
    }

    #[test]
    fn decrease_down_realistic_multi_round_last_users_get_not_zero() {
        let tick = 0;
        let initial_order_phase = 100;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0);

        // 200 users, 1,000 tokens each = 200,000 total
        // (e.g. 0.001 SOL per order with 9 decimals, or $1 USDC with 6 decimals)
        let order_count = 200usize;
        let order_size = 1_000u64;
        let mut orders: Vec<LimitOrderState> = (0..order_count)
            .map(|_| {
                open_order(
                    order_size,
                    limit_zero_for_one,
                    initial_order_phase,
                    &mut tick_state,
                )
            })
            .collect();

        // 10 rounds of swaps with non-round amounts
        // Total consumed: ~194,079 out of 200,000 (≈97%)
        // Each round fills a decreasing chunk: mimics real trading activity
        let swap_amounts: [u64; 10] = [
            67_123, 41_288, 28_342, 19_756, 13_457, 8_975, 6_124, 4_182, 2_857, 1_975,
        ];

        for &swap_amount in &swap_amounts {
            let avail = tick_state.limit_order_unfilled_amount().unwrap();
            if swap_amount > avail || avail == 0 {
                break;
            }
            tick_state
                .match_limit_order(swap_amount, !limit_zero_for_one, false, 0, true)
                .unwrap();

            // Between each round, all users settle (claiming output tokens)
            // Each settle_up introduces up to +1 ceil error per order
            for order in orders.iter_mut() {
                let _ = order.settle_filled_order(&tick_state).unwrap();
            }
        }

        let part_remaining = tick_state.part_filled_orders_remaining;

        // Compute sum of all orders' perceived unfilled amounts
        let sum_unfilled: u64 = orders
            .iter()
            .map(|o| o.get_unfilled_amount().unwrap())
            .sum();

        // Key: ceil rounding inflates sum_unfilled above actual part_remaining
        let rounding_gap = part_remaining - sum_unfilled;
        assert!(rounding_gap > 0,);

        // All 200 users try to withdraw remaining principal
        let mut zero_count = 0u64;

        for order in orders.iter_mut() {
            let unfilled_before = order.get_unfilled_amount().unwrap();
            let result = order.decrease_amount(&mut tick_state, order_size).unwrap();

            if unfilled_before > 0 && result.real_decrease_amount == 0 {
                zero_count += 1;
            }
        }
        assert!(zero_count == 0,);
    }
}
