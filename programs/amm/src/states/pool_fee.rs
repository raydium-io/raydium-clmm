use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

// Maximum fee rate numerator, 10%
pub const MAX_FEE_RATE_NUMERATOR: u32 = 100_000;

// Scale factor for the on-chain `volatility_accumulator` unit.
// `volatility_accumulator` is updated as: `volatility_reference + index_delta * SCALE`,
// then later decayed into a new `volatility_reference` via `reduction_factor`.
// If we used a raw integer `index_delta` (e.g. 1), repeated decay would quickly round it down to 0.
// Scaling 1 to 10_000 keeps enough granularity (e.g. 0.5 decay => 5_000) while staying integer-only.
pub const VOLATILITY_ACCUMULATOR_SCALE: u16 = 10_000;

// Fixed-point denominator for `reduction_factor` (a ratio in [0, 1)).
// For example, `reduction_factor = 5_000` represents 0.5.
pub const REDUCTION_FACTOR_DENOMINATOR: u16 = 10_000;

// Fixed-point denominator for `dynamic_fee_control`, which scales how volatility maps into fee rate.
// The model uses the (scaled) volatility accumulator (quadratic growth), so larger control values
// make fees ramp faster for the same volatility, and smaller values make fees ramp more gently.
// For example, `dynamic_fee_control = 1_000` represents 0.01.
pub const DYNAMIC_FEE_CONTROL_DENOMINATOR: u32 = 100_000;

/// Dynamic fee information for pool configuration
#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Debug, PartialEq, Eq)]
pub struct DynamicFeeInfo {
    /// Period that determines the high frequency trading time window (in seconds).
    pub filter_period: u16,
    /// Period that determines when the dynamic fee starts to decrease (in seconds).
    pub decay_period: u16,
    /// Dynamic fee rate decrement rate, used for volatility reference decay.
    pub reduction_factor: u16,
    /// Factor used to scale the dynamic fee component in the fee rate calculation.
    pub dynamic_fee_control: u32,
    /// Maximum value for the volatility accumulator, used to cap the dynamic fee rate.
    pub max_volatility_accumulator: u32,

    /// Active tick spacing index at the last reference update.
    pub tick_spacing_index_reference: i32,
    /// Volatility reference value, stores the decayed volatility accumulator.
    pub volatility_reference: u32,
    /// Volatility accumulator, used to calculate the dynamic fee rate.
    pub volatility_accumulator: u32,
    /// Last timestamp (block time) when `volatility_reference` and `tick_spacing_index_reference` were updated.
    pub last_update_timestamp: u64,
    /// Reserved for future upgrades.
    pub padding: [u8; 46],
}

impl Default for DynamicFeeInfo {
    fn default() -> Self {
        Self {
            filter_period: 0,
            decay_period: 0,
            reduction_factor: 0,
            dynamic_fee_control: 0,
            max_volatility_accumulator: 0,
            last_update_timestamp: 0,
            volatility_reference: 0,
            tick_spacing_index_reference: 0,
            volatility_accumulator: 0,
            padding: [0u8; 46],
        }
    }
}

impl DynamicFeeInfo {
    pub const LEN: usize = 2 + 2 + 2 + 4 + 4 + 4 + 4 + 4 + 8 + 1 * 46; // 80

    pub fn validate_params(
        tick_spacing: u16,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        dynamic_fee_control: u32,
        max_volatility_accumulator: u32,
    ) -> bool {
        if filter_period == 0 {
            return false;
        }
        if decay_period == 0 || decay_period <= filter_period {
            return false;
        }
        if reduction_factor == 0 || reduction_factor >= REDUCTION_FACTOR_DENOMINATOR {
            return false;
        }
        if dynamic_fee_control == 0 || dynamic_fee_control >= DYNAMIC_FEE_CONTROL_DENOMINATOR {
            return false;
        }
        // To ensure safety in dynamic fee calculation, max_volatility_accumulator * tick_spacing must not cause an integer overflow
        if u64::from(max_volatility_accumulator) * u64::from(tick_spacing) > u32::MAX as u64 {
            return false;
        }
        true
    }

    /// Updates the volatility accumulator based on the distance from the reference tick spacing index.
    ///
    /// The volatility accumulator measures the cumulative price movement since
    /// the last reference update. It is calculated as:
    /// `volatility_accumulator = volatility_reference + index_delta * VOLATILITY_ACCUMULATOR_SCALE`
    ///
    /// The accumulator is capped at `max_volatility_accumulator` to prevent excessive fee rates.
    pub fn update_volatility_accumulator(&mut self, tick_spacing_index: i32) -> Result<()> {
        // Calculate the absolute distance in tick groups from the reference point
        let index_delta = (self.tick_spacing_index_reference - tick_spacing_index).unsigned_abs();
        let volatility_accumulator = u64::from(self.volatility_reference)
            + u64::from(index_delta) * u64::from(VOLATILITY_ACCUMULATOR_SCALE);

        // Clamp to maximum value to prevent excessive fee rates
        self.volatility_accumulator = std::cmp::min(
            volatility_accumulator,
            u64::from(self.max_volatility_accumulator),
        ) as u32;

        Ok(())
    }

    /// Updates the volatility reference and tick spacing index reference based on time windows.
    ///
    /// This function implements a time-based decay mechanism for dynamic fee calculation.
    /// The volatility reference is used as a baseline for calculating volatility accumulator,
    /// which influences the dynamic fee rate. The update behavior depends on the time elapsed
    /// since the last reference update or major swap:
    ///
    /// 1. **High frequency period** (< `filter_period`): No update to prevent excessive fee changes
    /// 2. **Decay period** (>= `filter_period` and < `decay_period`): Update with decayed volatility
    /// 3. **Reset period** (>= `decay_period`): Reset volatility reference to 0
    pub fn update_reference(
        &mut self,
        tick_spacing_index: i32,
        current_timestamp: u64,
    ) -> Result<()> {
        let time_since_reference_update =
            current_timestamp.saturating_sub(self.last_update_timestamp);

        if time_since_reference_update < self.filter_period as u64 {
            // High frequency trading period: no update to prevent excessive fee changes
        } else if time_since_reference_update < self.decay_period as u64 {
            // Decay period: update references with decayed volatility
            self.tick_spacing_index_reference = tick_spacing_index;
            self.volatility_reference =
                (u64::from(self.volatility_accumulator) * u64::from(self.reduction_factor)
                    / u64::from(REDUCTION_FACTOR_DENOMINATOR)) as u32;
            self.last_update_timestamp = current_timestamp;
        } else {
            // Out of decay time window: reset volatility reference to 0
            self.tick_spacing_index_reference = tick_spacing_index;
            self.volatility_reference = 0;
            self.last_update_timestamp = current_timestamp;
        }

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum CollectFeeOn {
    /// The fee is collected from the input token.
    FromInput,
    /// The fee is collected from token0.
    Token0Only,
    /// The fee is collected from token1.
    Token1Only,
}

impl Default for CollectFeeOn {
    fn default() -> Self {
        Self::FromInput
    }
}

impl CollectFeeOn {
    pub fn to_u8(&self) -> u8 {
        match self {
            CollectFeeOn::FromInput => 0u8,
            CollectFeeOn::Token0Only => 1u8,
            CollectFeeOn::Token1Only => 2u8,
        }
    }
}

impl PoolState {
    pub fn initialize_dynamic_fee_info(
        &mut self,
        init_tick: i32,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        dynamic_fee_control: u32,
        max_volatility_accumulator: u32,
    ) -> Result<()> {
        if !DynamicFeeInfo::validate_params(
            self.tick_spacing,
            filter_period,
            decay_period,
            reduction_factor,
            dynamic_fee_control,
            max_volatility_accumulator,
        ) {
            return Err(ErrorCode::InvalidDynamicFeeConfigParams.into());
        }
        self.dynamic_fee_info = DynamicFeeInfo::default();
        self.dynamic_fee_info.filter_period = filter_period;
        self.dynamic_fee_info.decay_period = decay_period;
        self.dynamic_fee_info.reduction_factor = reduction_factor;
        self.dynamic_fee_info.dynamic_fee_control = dynamic_fee_control;
        self.dynamic_fee_info.max_volatility_accumulator = max_volatility_accumulator;
        self.dynamic_fee_info.tick_spacing_index_reference =
            tick_spacing_index_from_tick(init_tick, self.tick_spacing);

        Ok(())
    }

    pub fn get_dynamic_fee_info(&self) -> Option<DynamicFeeInfo> {
        if self.dynamic_fee_info == DynamicFeeInfo::default() {
            return None;
        }
        Some(self.dynamic_fee_info)
    }

    pub fn update_dynamic_fee_variables(
        &mut self,
        dynamic_fee_info: Option<DynamicFeeInfo>,
    ) -> Result<()> {
        if self.dynamic_fee_info == DynamicFeeInfo::default() {
            return Ok(());
        }

        match dynamic_fee_info {
            Some(info) => {
                self.dynamic_fee_info.last_update_timestamp = info.last_update_timestamp;
                self.dynamic_fee_info.volatility_reference = info.volatility_reference;
                self.dynamic_fee_info.tick_spacing_index_reference =
                    info.tick_spacing_index_reference;
                self.dynamic_fee_info.volatility_accumulator = info.volatility_accumulator;
            }
            None => {}
        }

        Ok(())
    }

    /// Get fee on which token (0 = FromInput, 1 = Token0Only, 2 = Token1Only)
    pub fn fee_on(&self) -> u8 {
        self.fee_on
    }

    pub fn set_fee_on(&mut self, fee_on: u8) -> Result<()> {
        require!(fee_on <= 2, ErrorCode::InvalidFeeOn);
        self.fee_on = fee_on;
        Ok(())
    }

    /// Determine if fee should be collected from input token
    pub fn is_fee_on_input(&self, zero_for_one: bool) -> bool {
        match self.fee_on() {
            0 => true,
            1 => zero_for_one,
            2 => !zero_for_one,
            _ => true, // default to FromInput
        }
    }

    /// Determine if fees should be collected from token0
    pub fn is_fee_on_token0(&self, zero_for_one: bool) -> bool {
        match self.fee_on() {
            0 => zero_for_one,
            1 => true,
            2 => false,
            _ => zero_for_one,
        }
    }
}

pub fn tick_spacing_index_from_tick(tick_index: i32, tick_spacing: u16) -> i32 {
    let tick_spacing = i32::from(tick_spacing);
    if tick_index % tick_spacing == 0 || tick_index >= 0 {
        tick_index / tick_spacing
    } else {
        tick_index / tick_spacing - 1
    }
}

#[cfg(test)]
mod pool_fee_test {
    use super::tick_spacing_index_from_tick;

    #[test]
    fn test_tick_spacing_index_from_tick() {
        let tick_spacings = [1u16, 10u16, 60u16];

        for &spacing in &tick_spacings {
            // Zero value
            assert_eq!(tick_spacing_index_from_tick(0, spacing), 0);

            // Positive ticks divisible by tick_spacing
            assert_eq!(tick_spacing_index_from_tick(spacing as i32, spacing), 1);
            assert_eq!(
                tick_spacing_index_from_tick((spacing * 2) as i32, spacing),
                2
            );
            assert_eq!(
                tick_spacing_index_from_tick((spacing * 10) as i32, spacing),
                10
            );

            // Positive ticks NOT divisible by tick_spacing (signum matches)
            if spacing > 1 {
                assert_eq!(tick_spacing_index_from_tick(1, spacing), 0);
                assert_eq!(tick_spacing_index_from_tick(spacing as i32 / 2, spacing), 0);
                assert_eq!(
                    tick_spacing_index_from_tick((spacing * 2 + 1) as i32, spacing),
                    2
                );
                assert_eq!(
                    tick_spacing_index_from_tick(
                        (spacing as i32) * 2 + (spacing as i32) / 2,
                        spacing
                    ),
                    2
                );
            }

            // Negative ticks divisible by tick_spacing
            assert_eq!(tick_spacing_index_from_tick(-(spacing as i32), spacing), -1);
            assert_eq!(
                tick_spacing_index_from_tick(-((spacing * 2) as i32), spacing),
                -2
            );
            assert_eq!(
                tick_spacing_index_from_tick(-((spacing * 10) as i32), spacing),
                -10
            );

            // Negative ticks NOT divisible by tick_spacing (signum doesn't match)
            if spacing > 1 {
                assert_eq!(tick_spacing_index_from_tick(-1, spacing), -1);
                assert_eq!(
                    tick_spacing_index_from_tick(-(spacing as i32 / 2), spacing),
                    -1
                );
                assert_eq!(
                    tick_spacing_index_from_tick(-((spacing * 2 + 1) as i32), spacing),
                    -3
                );
                assert_eq!(
                    tick_spacing_index_from_tick(
                        -((spacing as i32) * 2 + (spacing as i32) / 2),
                        spacing
                    ),
                    -3
                );
            }

            // Edge cases for spacing = 1
            if spacing == 1 {
                assert_eq!(tick_spacing_index_from_tick(1, spacing), 1);
                assert_eq!(tick_spacing_index_from_tick(-1, spacing), -1);
                assert_eq!(tick_spacing_index_from_tick(5, spacing), 5);
                assert_eq!(tick_spacing_index_from_tick(-5, spacing), -5);
                assert_eq!(tick_spacing_index_from_tick(1000, spacing), 1000);
                assert_eq!(tick_spacing_index_from_tick(-1000, spacing), -1000);
            }

            // Edge cases for spacing = 10
            if spacing == 10 {
                assert_eq!(tick_spacing_index_from_tick(1000, spacing), 100);
                assert_eq!(tick_spacing_index_from_tick(-1000, spacing), -100);
                // Test more specific cases
                assert_eq!(tick_spacing_index_from_tick(5, spacing), 0);
                assert_eq!(tick_spacing_index_from_tick(9, spacing), 0);
                assert_eq!(tick_spacing_index_from_tick(11, spacing), 1);
                assert_eq!(tick_spacing_index_from_tick(15, spacing), 1);
                assert_eq!(tick_spacing_index_from_tick(19, spacing), 1);
                assert_eq!(tick_spacing_index_from_tick(21, spacing), 2);
                assert_eq!(tick_spacing_index_from_tick(-5, spacing), -1);
                assert_eq!(tick_spacing_index_from_tick(-9, spacing), -1);
                assert_eq!(tick_spacing_index_from_tick(-11, spacing), -2);
                assert_eq!(tick_spacing_index_from_tick(-15, spacing), -2);
                assert_eq!(tick_spacing_index_from_tick(-19, spacing), -2);
                assert_eq!(tick_spacing_index_from_tick(-21, spacing), -3);
            }

            // Edge cases for spacing = 60
            if spacing == 60 {
                assert_eq!(tick_spacing_index_from_tick(30, spacing), 0);
                assert_eq!(tick_spacing_index_from_tick(90, spacing), 1);
                assert_eq!(tick_spacing_index_from_tick(120, spacing), 2);
                assert_eq!(tick_spacing_index_from_tick(121, spacing), 2);
                assert_eq!(tick_spacing_index_from_tick(-30, spacing), -1);
                assert_eq!(tick_spacing_index_from_tick(-90, spacing), -2);
                assert_eq!(tick_spacing_index_from_tick(-120, spacing), -2);
                assert_eq!(tick_spacing_index_from_tick(-121, spacing), -3);
            }
        }
    }
}
