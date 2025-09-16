use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateDynamicFeeConfig<'info> {
    #[account(
      mut,
      address = crate::admin::ID @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    #[account(init,
      payer = owner,
      seeds = [
        DYNAMIC_FEE_CONFIG_SEED.as_bytes(),
        &index.to_be_bytes(),
      ],
      bump,
      space = DynamicFeeConfig::LEN)]
    pub dynamic_fee_config: Account<'info, DynamicFeeConfig>,

    pub system_program: Program<'info, System>,
}

pub fn create_dynamic_fee_config(
    ctx: Context<CreateDynamicFeeConfig>,
    index: u16,
    filter_period: u16,
    decay_period: u16,
    reduction_factor: u16,
    dynamic_fee_control: u32,
    max_volatility_accumulator: u32,
) -> Result<()> {
    let dynamic_fee_config = &mut ctx.accounts.dynamic_fee_config;
    dynamic_fee_config.initialize(
        index,
        filter_period,
        decay_period,
        reduction_factor,
        dynamic_fee_control,
        max_volatility_accumulator,
    )?;
    Ok(())
}
