use crate::error::ErrorCode;
use crate::libraries::{big_num::U128, full_math::MulDiv};
use crate::libraries::{fixed_point_64, tick_math};
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
    /// Filled amount of the limit order
    pub filled_amount: u64,
    /// Order open time
    pub open_time: u64,
    pub padding: [u64; 6],
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DecreaseAmountResult {
    pub settled_output_amount: u64,
    pub real_decrease_amount: u64,
}

impl LimitOrderState {
    pub const LEN: usize = 8 + 32 + 32 + 4 + 1 + 8 + 8 + 8 + 8 + 8 * 6;

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
        self.padding = [0; 6];
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

    /// Settle order, return the amount of output token
    pub fn settle_filled_order(&mut self, tick_state: &TickState) -> Result<u64> {
        let remaining_amount = self.get_unfilled_amount()?;
        if remaining_amount == 0 {
            return Ok(0);
        }

        let filled_amount = if self.order_phase == tick_state.order_phase {
            // same phase, unfilled - no output
            0
        } else if self.order_phase + 1 == tick_state.order_phase {
            // Next phase, part filled: tick was partially matched during swap.
            // Estimate this order's unfilled portion by scaling: unfilled_ratio = remaining / total on tick.
            require_gt!(tick_state.part_filled_orders_total, 0);
            // Use ceil to avoid over-attributing fills (protocol safety: don't over-pay output).
            // new_remaining_amount = ceil(total * part_remaining / part_total) = conservative estimate.
            let new_remaining_amount = U128::from(self.total_amount)
                .mul_div_ceil(
                    U128::from(tick_state.part_filled_orders_remaining),
                    U128::from(tick_state.part_filled_orders_total),
                )
                .ok_or(ErrorCode::CalculateOverflow)?
                .as_u64();
            // filled_amount = remaining - new_remaining (amount matched in this part-fill)
            remaining_amount.saturating_sub(new_remaining_amount)
        } else if self.order_phase + 2 <= tick_state.order_phase {
            // full filled
            remaining_amount
        } else {
            return err!(ErrorCode::InvalidOrderPhase);
        };

        self.filled_amount = self
            .filled_amount
            .checked_add(filled_amount)
            .ok_or(ErrorCode::CalculateOverflow)?;

        // Calculate the amount of tokens received after selling via limit order
        let amount_out =
            TickState::get_limit_order_output(filled_amount, tick_state.tick, self.zero_for_one)?;
        Ok(amount_out)
    }

    pub fn increase_amount(&mut self, tick_state: &mut TickState, amount: u64) -> Result<()> {
        if self.order_phase != tick_state.order_phase {
            return err!(ErrorCode::InvalidOrderPhase);
        }
        if self.filled_amount > 0 {
            return err!(ErrorCode::OrderAlreadyFilled);
        }

        self.total_amount = self
            .total_amount
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
        let settled_output_amount = self.settle_filled_order(tick_state)?;
        let unfilled_amount = self.get_unfilled_amount()?;
        if unfilled_amount == 0 {
            return Ok(DecreaseAmountResult {
                settled_output_amount,
                real_decrease_amount: 0,
            });
        }
        let real_decrease_amount = amount.min(unfilled_amount);
        if self.order_phase == tick_state.order_phase {
            require_eq!(unfilled_amount, self.total_amount);
            tick_state.orders_amount = tick_state
                .orders_amount
                .checked_sub(real_decrease_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else if self.order_phase + 1 == tick_state.order_phase {
            tick_state.part_filled_orders_remaining = tick_state
                .part_filled_orders_remaining
                .checked_sub(real_decrease_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;
            tick_state.part_filled_orders_total = tick_state
                .part_filled_orders_total
                .checked_sub(real_decrease_amount)
                .ok_or(ErrorCode::CalculateOverflow)?;
        } else {
            return err!(ErrorCode::OrderAlreadyFilled);
        };
        self.total_amount = self
            .total_amount
            .checked_sub(real_decrease_amount)
            .ok_or(ErrorCode::CalculateOverflow)?;

        let remaining_amount = self.get_unfilled_amount()?;
        if remaining_amount != 0 {
            let min_output_amount = if self.zero_for_one {
                let token_0_price_x64 = tick_math::get_price_at_tick(tick_state.tick, false)?;
                U128::from(remaining_amount)
                    .mul_div_floor(token_0_price_x64, U128::from(fixed_point_64::Q64))
                    .ok_or(ErrorCode::CalculateOverflow)?
                    .as_u64()
            } else {
                let token_0_price_x64 = tick_math::get_price_at_tick(tick_state.tick, true)?;
                U128::from(remaining_amount)
                    .mul_div_floor(U128::from(fixed_point_64::Q64), token_0_price_x64)
                    .ok_or(ErrorCode::CalculateOverflow)?
                    .as_u64()
            };
            require_gte!(min_output_amount, 1, ErrorCode::InvalidLimitOrderAmount);
        };

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
    /// Amount of output tokens transferred (excluding fees)
    pub settled_output_amount: u64,
    /// Decreased amount of the limit order
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
        part_filled_orders_total: u64,
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
            part_filled_orders_total,
            part_filled_orders_remaining,
            padding: [0; 5],
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

    #[test]
    fn order_increase_test() {
        // Test order modification (increase amount)
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        // Increase order amount
        let result = order.increase_amount(&mut tick_state, 500);
        assert!(result.is_ok());
        assert_eq!(order.total_amount, 1500); // 1000 + 500 = 1500 (increased)

        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.part_filled_orders_remaining == 1000);
        assert!(tick_state.part_filled_orders_total == 1500);
        assert!(tick_state.order_phase == initial_order_phase + 1); // Order phase advances when consuming from orders_amount

        // Try to increase order amount after partially filled
        // not allowed, will return error
        let result = order.increase_amount(&mut tick_state, 500);
        assert!(result.is_err());
    }

    #[test]
    fn order_decrease_test() {
        // Test order modification (decrease amount)
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Decrease order amount, not filled
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 300).unwrap();
        assert!(settled_output_amount == 0);
        assert!(real_decrease_amount == 300);
        assert_eq!(order.total_amount, 700);

        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.part_filled_orders_remaining == 600);
        assert!(tick_state.part_filled_orders_total == 700);
        assert!(tick_state.order_phase == initial_order_phase + 1);

        // Decrease order amount after partially filled
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 300).unwrap();
        assert!(settled_output_amount == 100);
        assert!(real_decrease_amount == 300);
        assert_eq!(order.total_amount, 400); // 700 - 300 = 400
        assert!(tick_state.part_filled_orders_remaining == 300);
        assert!(tick_state.part_filled_orders_total == 400);

        let result = tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert!(result.amount_out == 300);
        assert!(tick_state.part_filled_orders_remaining == 0);

        // Decrease order amount after full filled
        // Only settle, do not decrease the order amount
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 200).unwrap();
        assert_eq!(settled_output_amount, 300);
        assert_eq!(real_decrease_amount, 0);
        assert_eq!(order.filled_amount, 400);
        assert_eq!(order.total_amount, 400);
        assert!(tick_state.order_phase == order.order_phase + 1);
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.part_filled_orders_remaining == 0);
        assert!(tick_state.part_filled_orders_total == 400);
    }

    #[test]
    fn decrease_amount_exceeds_available_test() {
        // Test that decreasing more than available amount clamps to available
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 1000, 0, 0);

        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        // Try to decrease more than available
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
        // Test handling of fully filled orders
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // Test settlement when order is fully filled
        let output = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output, 1000);
        assert!(order.is_fully_filled());

        // Test that increase a fully filled order fails
        let result = order.increase_amount(&mut tick_state, 100);
        assert!(result.is_err());

        // Test that decrease a fully filled order fails
        let DecreaseAmountResult {
            settled_output_amount,
            real_decrease_amount,
        } = order.decrease_amount(&mut tick_state, 100).unwrap();
        assert_eq!(settled_output_amount, 0);
        assert_eq!(real_decrease_amount, 0);
    }

    #[test]
    fn multi_order_settlement_by_order_phase_test() {
        // Test multiple orders settlement based on order_phase differences
        // Use a simple test that focuses on order_phase logic without complex price calculations
        let tick = 0;
        let initial_order_phase = 1000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order1 = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );

        // Mock the get_limit_order_output to return 0 for unfilled orders
        let order1_output1 = order1.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order1_output1, 0); // Should be unfilled

        let mut order2 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );
        let order2_output1 = order2.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order2_output1, 0); // Should be unfilled

        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // order.order_phase == tick_state.order_phase (partially filled)
        let mut order3 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Simulate the transaction, but order1 and order2 have not been fully consumed yet
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Should be unfilled
        let order3_output1 = order3.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order3_output1, 0);

        // Should be partially filled
        let order1_output2 = order1.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order1_output2, 500);

        // Should be partially filled
        let order2_output2 = order2.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order2_output2, 500);

        // Simulate the transaction again, just enough to exhaust order1 and order2
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // There should not have been any filed before the settlement of order1 and order2.
        let order3_output1 = order3.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order3_output1, 0); // Should be unfilled

        // Should be partially filled
        let order1_output2 = order1.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order1_output2, 500);

        // Should be partially filled
        let order2_output2 = order2.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order2_output2, 500);

        // Simulate the transaction again, order3 should be partially filled
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Should be partially filled
        let order1_output2 = order1.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order1_output2, 0);

        // Should be partially filled
        let order2_output2 = order2.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order2_output2, 0);

        let order3_output1 = order3.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(order3_output1, 500); // Should be partially filled
    }

    #[test]
    fn fifo_order_priority_preservation_test() {
        // Test that order priority is preserved across price movements
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        // Initial state: some orders partially filled
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 300, 1000, 200);

        // Test that part_filled_orders_remaining gets absolute priority
        let result = tick_state
            .match_limit_order(150, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 150);
        assert!(tick_state.part_filled_orders_total == 1000);
        assert!(tick_state.part_filled_orders_remaining == 50); // 200 - 150
        assert!(tick_state.orders_amount == 300); // Untouched

        // Test that new orders wait until part_filled is completely consumed
        let result2 = tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result2.amount_out, 100);

        assert!(tick_state.part_filled_orders_total == 300);
        assert!(tick_state.part_filled_orders_remaining == 250); // 50 remaining + (300 - 100) = 250 moved to part_filled_orders_remaining
        assert!(tick_state.orders_amount == 0); // All consumed
    }

    #[test]
    fn fifo_edge_case_empty_part_filled_test() {
        // Test edge case when part_filled_orders_remaining is 0
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;
        // No part filled orders
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 500, 0, 0);

        // Should consume from orders_amount directly
        let result = tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 300);
        let part_remaining = tick_state.part_filled_orders_remaining;
        let limit_amount = tick_state.orders_amount;
        let order_phase = tick_state.order_phase;
        assert_eq!(part_remaining, 200); // 500 - 300 = 200 moved to part_filled_orders_remaining
        assert_eq!(limit_amount, 0); // orders_amount consumed and moved to part_filled_orders_remaining
        assert_eq!(order_phase, initial_order_phase + 1); // Order phase advances when consuming from orders_amount
        assert!(tick_state.part_filled_orders_total == 500);
    }

    #[test]
    fn part_filled_orders_total_proportional_settlement_test() {
        // Test that part_filled_orders_total is used correctly in proportional order settlement
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 1000, 300);

        // Create an order that should be partially filled
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            initial_order_phase,
            &mut tick_state,
        );
        tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let output = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output, 0);

        tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let output = order.settle_filled_order(&mut tick_state).unwrap();

        // The order should be filled proportionally:
        // new_remaining_amount = (1000 * 300) / 1000 = 300
        // filled_amount = 1000 - 300 = 700
        assert_eq!(output, 300);
        assert_eq!(order.filled_amount, 300);
        assert!(tick_state.part_filled_orders_total == 1000);
        assert!(tick_state.part_filled_orders_remaining == 700);
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.order_phase == initial_order_phase + 1);
    }

    #[test]
    fn part_filled_orders_total_complete_consumption_test() {
        // Test the complete consumption of part_filled_orders_remaining
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;
        // Set up scenario with remaining part filled orders
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 1000, 200);

        // Consume all remaining part filled orders
        let result = tick_state
            .match_limit_order(200, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 200);

        let part_total = tick_state.part_filled_orders_total;
        let part_remaining = tick_state.part_filled_orders_remaining;
        let order_phase = tick_state.order_phase;

        // part_filled_orders_total should be moved to fulfilled
        assert_eq!(part_total, 1000);
        assert_eq!(part_remaining, 0);
        // order_phase should remain the same when part_filled_orders_remaining is completely consumed
        assert_eq!(order_phase, initial_order_phase);
    }

    #[test]
    fn part_filled_orders_total_order_settlement_accuracy_test() {
        // Test that order settlement calculations are accurate with part_filled_orders_total
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        // Create multiple orders with different amounts
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

        tick_state
            .match_limit_order(800, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Settle all orders and verify proportional filling
        let mut total_filled = 0;
        for order in &mut orders {
            let unfilled_amount = order.get_unfilled_amount().unwrap();
            let output = order.settle_filled_order(&mut tick_state).unwrap();
            total_filled += output;

            let new_remaining_amount = (unfilled_amount * 200) / 1000;
            let expected_fill = order.total_amount - new_remaining_amount;
            assert_eq!(output, expected_fill);
        }

        // Total filled should be 80% of total order amounts (1000) = 800
        assert_eq!(total_filled, 800);
    }

    #[test]
    fn multi_user_orders_test() {
        // Scenario: Multiple users place orders at different order_phases, price moves, orders get filled
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;

        // Initial state - no orders
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);

        // User A places order (order_phase 0)
        let mut user_a_order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        //  User B places order (order_phase 0)
        let mut user_b_order = open_order(
            2000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Price moves, order_phase advances to 1
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // User C places new order (order_phase 1)
        let mut user_c_order = open_order(
            1500,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Test settlement of User A's order (order_phase 0, should be partially filled)
        let output_a = user_a_order.settle_filled_order(&mut tick_state).unwrap();
        // Should be filled proportionally: ceil((1000 * 2500) / 3000) = 834 remaining, so 166 filled
        assert_eq!(output_a, 166);

        //  Test settlement of User B's order (order_phase 0, should be partially filled)
        let output_b = user_b_order.settle_filled_order(&mut tick_state).unwrap();
        // Should be filled proportionally: ceil((2000 * 2500) / 3000) = 1667 remaining, so 333 filled
        assert_eq!(output_b, 333);

        //  Test settlement of User C's order (order_phase 1, should be unfilled)
        let output_c = user_c_order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output_c, 0); // Same order_phase, unfilled

        //  Orders from User A and User B have just been fully filled, while User C's order has not been filled yet
        tick_state
            .match_limit_order(2500, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let output_a = user_a_order.settle_filled_order(&mut tick_state).unwrap();
        // Should be filled proportionally: (833 * 0) / 3000 = 0 remaining, so 833 filled
        assert_eq!(output_a, 834);

        //  Test settlement of User B's order (order_phase 0, should be partially filled)
        let output_b = user_b_order.settle_filled_order(&mut tick_state).unwrap();
        // Should be filled proportionally: (1666 * 0) / 3000 = 0 remaining, so 1666 filled
        assert_eq!(output_b, 1667);

        //  Test settlement of User C's order (order_phase 1, should be unfilled)
        let output_c = user_c_order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output_c, 0); // Same order_phase, unfilled

        // User C's order should now be partially filled
        tick_state
            .match_limit_order(1, !limit_zero_for_one, false, 0, true)
            .unwrap();

        let output_c_again = user_c_order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output_c_again, 1); // Should be partially filled now

        // User C's order should be fully filled
        tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let output_c_final = user_c_order.settle_filled_order(&mut tick_state).unwrap();
        // Should be partially filled based on new liquidity
        assert_eq!(output_c_final, 1499);
    }

    #[test]
    fn order_modifications_test() {
        // Scenario: Users modify their orders (increase/decrease) at different stages
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = false; // Testing opposite direction

        // Step 1: User places initial order
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);

        let mut user_order = open_order(
            5000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Step 2: User increases order (same order_phase, should work)
        let result_increase = user_order.increase_amount(&mut tick_state, 2000);
        assert!(result_increase.is_ok());
        assert_eq!(user_order.total_amount, 7000); // 5000 + 2000
        let orders_amount_after_increase = tick_state.orders_amount;
        assert_eq!(orders_amount_after_increase, 7000); // Updated in tick state

        // Step 3: User decreases order (same order_phase, should work)
        let result_decrease = user_order.decrease_amount(&mut tick_state, 1000);
        assert!(result_decrease.is_ok());
        assert_eq!(user_order.total_amount, 6000); // 7000 - 1000
        let orders_amount_after_decrease = tick_state.orders_amount;
        assert_eq!(orders_amount_after_decrease, 6000); // Updated in tick state

        // Step 4: Order phase advances, price moves, some liquidity gets partially filled
        tick_state
            .match_limit_order(2000, !limit_zero_for_one, true, 0, true)
            .unwrap();

        // Step 5: Try to increase order (should fail - order is partially filled)
        let result_increase_fail = user_order.increase_amount(&mut tick_state, 1000);
        assert!(result_increase_fail.is_err());

        // Step 6: Decrease order (should work - decrease from unfilled amount)
        let result_decrease_ok = user_order.decrease_amount(&mut tick_state, 2000);
        assert!(result_decrease_ok.is_ok());
        assert_eq!(user_order.total_amount, 4000); // 6000 - 2000
        let part_filled_remaining_after_decrease = tick_state.part_filled_orders_remaining;
        assert_eq!(part_filled_remaining_after_decrease, 2000); // Updated

        // Step 7: Try to decrease more than unfilled amount (should ok, but nothing will be decreased)
        let result_decrease_ok = user_order.decrease_amount(&mut tick_state, 3000);
        assert!(result_decrease_ok.is_ok());
    }

    #[test]
    fn mult_order_settlement_test() {
        // Scenario: Price oscillates back and forth, testing FIFO priority
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);

        // Step 1: Initial orders
        let mut old_orders = Vec::new();
        for _i in 0..5 {
            let order = open_order(
                1000,
                limit_zero_for_one,
                tick_state.order_phase,
                &mut tick_state,
            );
            old_orders.push(order);
        }

        // Step 2: Price moves up, order_phase advances, 2000 consumed
        tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();

        // Step 3: New orders come in at higher price
        let mut new_orders = Vec::new();
        for _i in 0..2 {
            let order = open_order(
                1000,
                limit_zero_for_one,
                tick_state.order_phase,
                &mut tick_state,
            );
            new_orders.push(order);
        }

        // Test that old orders (part_filled_orders_remaining) have priority
        let result = tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 1000);

        // Should consume from part_filled_orders_remaining first
        assert!(tick_state.part_filled_orders_remaining == 2000);
        assert!(tick_state.part_filled_orders_total == 5000);
        assert!(tick_state.orders_amount == 2000);

        // Step 5: Consume more - should still prioritize old orders
        let result2 = tick_state
            .match_limit_order(2000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result2.amount_out, 2000);

        assert!(tick_state.part_filled_orders_remaining == 0);
        assert!(tick_state.orders_amount == 2000);
        assert!(tick_state.part_filled_orders_total == 5000);

        // Step 6: Test order settlements
        // Old orders should be fully filled (order_phase 0 + 2 <= 2)
        for order in &mut old_orders {
            let output = order.settle_filled_order(&mut tick_state).unwrap();
            // The output might be slightly different due to price calculations
            assert_eq!(output, 1000);
        }

        // New orders should be partially filled (order_phase 1 + 1 == 2)
        for order in &mut new_orders {
            let output = order.settle_filled_order(&mut tick_state).unwrap();
            // Should be partially filled, exact amount depends on calculation
            assert!(output == 0);
        }
    }

    #[test]
    fn match_limit_order_base_input_flow_test() {
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);

        // Open order A: 1000 (same order_phase)
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // First match: is_base_input = true, base is input, fee_rate > 0 triggers fee calculation
        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, 100, true)
            .unwrap();
        // Basic sanity: consume orders_amount and enter partial-filled cohort
        assert_eq!(res1.amount_in, 499);
        assert_eq!(res1.amount_out, 499);
        assert!(res1.amm_fee_amount == 1);
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.part_filled_orders_total == 1000);
        assert!(tick_state.part_filled_orders_remaining == 501);

        // Second match: continue to consume the remaining partial
        let res2 = tick_state
            .match_limit_order(1000, !limit_zero_for_one, true, 100, true)
            .unwrap();
        assert_eq!(res2.amount_in, 501);
        assert_eq!(res2.amount_out, 501);
        assert!(tick_state.part_filled_orders_remaining == 0);

        let out_settle_1 = order.settle_filled_order(&mut tick_state).unwrap();

        assert_eq!(out_settle_1, 1000);
        assert!(order.is_fully_filled());

        // Subsequent settlement yields no additional output
        let out_settle_2 = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(out_settle_2, 0);
    }

    #[test]
    fn partial_then_double_settle_before_second_match_test() {
        // Open order with amount 1000 at order_phase 0
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Step 1: Partially consume from orders_amount -> moves to part_filled cohort, order_phase advances to 1
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // Sanity after partial
        let total_after_partial = tick_state.part_filled_orders_total;
        let remaining_after_partial = tick_state.part_filled_orders_remaining;
        let orders_amount_after_partial = tick_state.orders_amount;
        let order_phase_after_partial = tick_state.order_phase;
        assert_eq!(total_after_partial, 1000);
        assert_eq!(remaining_after_partial, 500);
        assert_eq!(orders_amount_after_partial, 0);
        assert_eq!(order_phase_after_partial, initial_order_phase + 1);

        // First settle: with remaining=1000 and cohort remaining=500/1000 -> 500 filled
        let first_out = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(first_out, 500);
        assert_eq!(order.filled_amount, 500);

        // Second settle BEFORE any further match:
        // No new matching happened since last settle, so no additional fill should be realized.
        let second_out = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(second_out, 0);
        assert_eq!(order.filled_amount, 500);

        // Step 2: Further consume remaining
        tick_state
            .match_limit_order(100, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let remaining_after_second_match = tick_state.part_filled_orders_remaining;
        assert_eq!(remaining_after_second_match, 400);

        let third_out = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(third_out, 100);
        assert_eq!(order.filled_amount, 600);

        // Step 3: Further consume remaining cohort so that the order can be fully settled on third settle
        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        let remaining_after_second_match = tick_state.part_filled_orders_remaining;
        assert_eq!(remaining_after_second_match, 0);

        let third_out = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(third_out, 400);
        assert_eq!(order.filled_amount, 1000);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn match_limit_order_fee_from_output_test() {
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;
        let fee_rate = 100; // 0.01%

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Test 1: Exact Input Swap + fee from output
        // swap_amount = 500, fee_rate = 100
        // amount_in = 500 (no fee deducted from input)
        // amount_out (gross) = 500 (at tick 0, 1:1 ratio)
        // fee = 500 * 100 / 1_000_000 = 0.05, rounded up to 1
        // amount_out (net) = 500 - 1 = 499
        // Limit order consumption uses gross output (500), so remaining = 1000 - 500 = 500
        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, fee_rate, false)
            .unwrap();
        assert_eq!(res1.amount_in, 500);
        assert_eq!(res1.amount_out, 499); // net output after fee deduction
        assert_eq!(res1.amm_fee_amount, 1); // fee = 500 * 100 / 1_000_000 = 1 (rounded up)
        assert!(tick_state.orders_amount == 0);
        assert!(tick_state.part_filled_orders_total == 1000);
        assert!(tick_state.part_filled_orders_remaining == 500); // 1000 - 500 (gross output)

        // Test 2: Exact Output Swap + fee from output
        // swap_amount (desired net output) = 400
        // gross_output = ceil(400 * 1_000_000 / (1_000_000 - 100)) = ceil(400.04...) = 401
        // fee = ceil(401 * 100 / 1_000_000) = ceil(0.0401) = 1
        // amount_out (net) = 401 - 1 = 400
        let res2 = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();
        assert_eq!(res2.amount_out, 400); // net output should equal swap_amount (desired net output)
        assert_eq!(res2.amm_fee_amount, 1); // fee should be calculated
        let gross_output = res2.amount_out + res2.amm_fee_amount;
        assert!(tick_state.part_filled_orders_remaining == 500 - gross_output);

        // Test 3: Exact Input Swap + fee from output with larger amount
        // After Test 2, part_filled_orders_remaining = 500 - 401 = 99
        // swap_amount = 1000 (input amount), fee_rate = 100
        // amount_in = 1000 (no fee from input)
        // amount_out (gross) = 1000 (at tick 0, 1:1 ratio)
        // But total_unfilled = 99, so recalculate:
        //   amount_out (gross) = 99
        //   amount_in = get_limit_order_input(99, ...) (recalculated, may vary due to rounding)
        // fee = ceil(99 * 100 / 1_000_000) = ceil(0.0099) = 1
        // amount_out (net) = 99 - 1 = 98
        let res3 = tick_state
            .match_limit_order(1000, !limit_zero_for_one, true, fee_rate, false)
            .unwrap();
        // Verify fee from output logic: gross output = net output + fee
        assert_eq!(res3.amount_out, 98); // net output after fee deduction
        assert_eq!(res3.amm_fee_amount, 1); // fee = ceil(99 * 100 / 1_000_000) = 1
        assert_eq!(res3.amount_out + res3.amm_fee_amount, 99); // gross output = total_unfilled
        assert!(tick_state.part_filled_orders_remaining == 0); // all consumed

        let out_settle = order.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(out_settle, 1000);
        assert!(order.is_fully_filled());
    }

    #[test]
    fn match_limit_order_is_base_input_false_fee_on_input_test() {
        // Test: is_base_input = false, is_fee_on_input = true
        // This combination is not well tested
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;
        let fee_rate = 100; // 0.01%

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // is_base_input = false: swap_amount is desired net output
        // is_fee_on_input = true: fee is calculated from input
        // swap_amount = 500 (desired net output)
        // result.amount_out = 500 (net_output, since is_fee_on_input)
        // result.amount_in = get_limit_order_input(500, ...)
        // fee = amount_in * fee_rate / (FEE_RATE_DENOMINATOR - fee_rate)
        let res = tick_state
            .match_limit_order(500, !limit_zero_for_one, false, fee_rate, true)
            .unwrap();
        assert_eq!(res.amount_out, 500); // net output equals swap_amount
        assert!(res.amm_fee_amount > 0); // fee should be calculated from input
        let part_total = tick_state.part_filled_orders_total;
        let part_remaining = tick_state.part_filled_orders_remaining;
        assert_eq!(part_total, 1000);
        assert_eq!(part_remaining, 500); // 1000 - 500
    }

    #[test]
    fn match_limit_order_consume_from_both_part_remaining_and_orders_amount_test() {
        // Test: consume from both part_filled_orders_remaining and orders_amount in one call
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        // Initial state: some part_filled_orders_remaining and some orders_amount
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 500, 1000, 200);

        // Consume more than part_filled_orders_remaining
        let result = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 400);

        // Should consume all part_filled_orders_remaining (200) first
        // Then consume from orders_amount (200)
        let part_remaining = tick_state.part_filled_orders_remaining;
        let part_total = tick_state.part_filled_orders_total;
        let orders_amount = tick_state.orders_amount;
        let order_phase = tick_state.order_phase;
        assert_eq!(part_remaining, 300); // 500 - 200 = 300
        assert_eq!(part_total, 500); // overwritten
        assert_eq!(orders_amount, 0);
        assert_eq!(order_phase, initial_order_phase + 1);
    }

    #[test]
    fn match_limit_order_exact_output_exceeds_total_unfilled_test() {
        // Test: is_base_input = false, swap_amount > total_unfilled_amount
        // Should clamp to total_unfilled_amount
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;
        let fee_rate = 100;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // First match: consume 500
        tick_state
            .match_limit_order(500, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();

        // Second match: request more than available (1500 > 500 remaining)
        // net_output = min(1500, 500) = 500
        // gross_output = ceil(500 * 1_000_000 / (1_000_000 - 100)) = 501
        // But then it's clamped: result.amount_out = min(501, 500) = 500
        // Then amount_in is recalculated from amount_out = 500
        // Fee is calculated from gross output (500) before fee deduction
        // Limit order consumption uses gross output (500), so remaining = 500 - 500 = 0
        let res = tick_state
            .match_limit_order(1500, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();
        // Should only consume what's available
        // Note: amount_out may be slightly less than 500 due to fee deduction
        assert!(res.amount_out <= 500); // net output should be <= 500
        assert!(res.amount_out >= 498); // allow for fee deduction
        assert!(res.amm_fee_amount > 0); // fee should be calculated
        let part_remaining = tick_state.part_filled_orders_remaining;
        assert_eq!(part_remaining, 0); // all consumed
    }

    #[test]
    fn match_limit_order_exact_input_exceeds_total_unfilled_recalculation_test() {
        // Test: is_base_input = true, calculated amount_out > total_unfilled_amount
        // Should recalculate amount_in and amount_out
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;
        let fee_rate = 100;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // First match: consume 500
        tick_state
            .match_limit_order(500, !limit_zero_for_one, true, fee_rate, true)
            .unwrap();

        // Second match: input amount that would produce output > remaining (500)
        // This should trigger recalculation
        // Initial: amount_in = 2000 - fee, amount_out = get_limit_order_output(amount_in, ...)
        // If amount_out > 500: amount_out = 500, amount_in = get_limit_order_input(500, ...)
        // Then fee recalculated from new amount_in
        let res = tick_state
            .match_limit_order(2000, !limit_zero_for_one, true, fee_rate, true)
            .unwrap();
        // Should recalculate: amount_out should be close to 500 (may vary due to rounding)
        assert!(res.amount_out <= 501); // allow small rounding differences
        assert!(res.amount_out >= 499);
        assert!(res.amount_in < 2000); // recalculated input should be less
        let part_remaining = tick_state.part_filled_orders_remaining;
        assert_eq!(part_remaining, 0);
    }

    #[test]
    fn match_limit_order_zero_fee_rate_test() {
        // Test: fee_rate = 0, should not affect calculations
        let tick = 0;
        let initial_order_phase = 0;
        let limit_zero_for_one = true;
        let fee_rate = 0;

        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Test with is_base_input = true, is_fee_on_input = true
        let res1 = tick_state
            .match_limit_order(500, !limit_zero_for_one, true, fee_rate, true)
            .unwrap();
        assert_eq!(res1.amm_fee_amount, 0);
        assert_eq!(res1.amount_in, 500);
        assert_eq!(res1.amount_out, 500); // at tick 0, 1:1 ratio

        // Test with is_base_input = false, is_fee_on_input = false
        let res2 = tick_state
            .match_limit_order(400, !limit_zero_for_one, false, fee_rate, false)
            .unwrap();
        assert_eq!(res2.amm_fee_amount, 0);
        assert_eq!(res2.amount_out, 400);
    }

    #[test]
    fn match_limit_order_part_filled_total_overwrite_after_complete_consumption_test() {
        // Test: When part_filled_orders_remaining becomes 0, and then new orders_amount is consumed,
        // part_filled_orders_total should be overwritten correctly
        let tick = 0;
        let initial_order_phase = 1000000;
        let limit_zero_for_one = true;

        // Step 1: Create order and partially fill it
        let mut tick_state = create_mock_tick_state(tick, initial_order_phase, 0, 0, 0);
        let mut order1 = open_order(
            1000,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        tick_state
            .match_limit_order(1000, !limit_zero_for_one, false, 0, true)
            .unwrap();
        // After this: part_filled_orders_remaining = 0, part_filled_orders_total = 1000

        // Step 2: Create new order at new phase
        let mut order2 = open_order(
            500,
            limit_zero_for_one,
            tick_state.order_phase,
            &mut tick_state,
        );

        // Step 3: Consume from new orders_amount
        // This should overwrite part_filled_orders_total to 500
        let result = tick_state
            .match_limit_order(300, !limit_zero_for_one, false, 0, true)
            .unwrap();
        assert_eq!(result.amount_out, 300);
        let part_total = tick_state.part_filled_orders_total;
        let part_remaining = tick_state.part_filled_orders_remaining;
        let orders_amount = tick_state.orders_amount;
        assert_eq!(part_total, 500); // overwritten
        assert_eq!(part_remaining, 200); // 500 - 300
        assert_eq!(orders_amount, 0);

        // Step 4: Verify order1 settlement still works correctly
        // order1 should be fully filled (order_phase + 2 <= tick_state.order_phase)
        let output1 = order1.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output1, 1000);
        assert!(order1.is_fully_filled());

        // Step 5: Verify order2 settlement works correctly
        // order2 should be partially filled (order_phase + 1 == tick_state.order_phase)
        let output2 = order2.settle_filled_order(&mut tick_state).unwrap();
        assert_eq!(output2, 300); // 500 - 200 (remaining) = 300 filled
    }
}
