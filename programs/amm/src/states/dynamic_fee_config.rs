use crate::error::ErrorCode;
use crate::states::DynamicFeeInfo;
use anchor_lang::prelude::*;

pub const DYNAMIC_FEE_CONFIG_SEED: &str = "dynamic_fee_config";
#[account]
pub struct DynamicFeeConfig {
    pub index: u16,
    // dynamic fee constants
    pub filter_period: u16,
    pub decay_period: u16,
    pub reduction_factor: u16,
    pub dynamic_fee_control: u32,
    pub max_volatility_accumulator: u32,
    // padding space for upgrade
    pub padding: [u64; 8],
}

impl DynamicFeeConfig {
    pub const LEN: usize = 8 + 2 + 2 + 2 + 2 + 4 + 4 + 8 * 8;

    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        &mut self,
        index: u16,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        dynamic_fee_control: u32,
        max_volatility_accumulator: u32,
    ) -> Result<()> {
        self.index = index;
        self.update_dynamic_fee_config(
            filter_period,
            decay_period,
            reduction_factor,
            dynamic_fee_control,
            max_volatility_accumulator,
        )?;

        Ok(())
    }

    pub fn update_dynamic_fee_config(
        &mut self,
        filter_period: u16,
        decay_period: u16,
        reduction_factor: u16,
        dynamic_fee_control: u32,
        max_volatility_accumulator: u32,
    ) -> Result<()> {
        if !DynamicFeeInfo::validate_params(
            1000,
            filter_period,
            decay_period,
            reduction_factor,
            dynamic_fee_control,
            max_volatility_accumulator,
        ) {
            return Err(ErrorCode::InvalidDynamicFeeConfigParams.into());
        }
        self.filter_period = filter_period;
        self.decay_period = decay_period;
        self.reduction_factor = reduction_factor;
        self.dynamic_fee_control = dynamic_fee_control;
        self.max_volatility_accumulator = max_volatility_accumulator;
        Ok(())
    }
}
