use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetProtocolFee<'info> {
    /// Valid protocol owner
    #[account(address = factory_state.owner)]
    pub owner: Signer<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub factory_state: Account<'info, FactoryState>,
}
