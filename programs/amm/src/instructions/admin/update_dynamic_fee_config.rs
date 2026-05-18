use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateDynamicFeeConfig<'info> {
    /// The admin has permission to update the config
    #[account(
        address = crate::admin::ID @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    /// The dynamic fee config account to be updated
    #[account(mut)]
    pub dynamic_fee_config: Account<'info, DynamicFeeConfig>,
}

pub fn update_dynamic_fee_config(
    ctx: Context<UpdateDynamicFeeConfig>,
    filter_period: u16,
    decay_period: u16,
    reduction_factor: u16,
    dynamic_fee_control: u32,
    max_volatility_accumulator: u32,
) -> Result<()> {
    let dynamic_fee_config = &mut ctx.accounts.dynamic_fee_config;
    dynamic_fee_config.update_dynamic_fee_config(
        filter_period,
        decay_period,
        reduction_factor,
        dynamic_fee_control,
        max_volatility_accumulator,
    )?;
    Ok(())
}
