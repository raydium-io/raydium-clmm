use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SetFeeProtocol<'info> {
    /// Valid protocol owner
    #[account(address = factory_state.load()?.owner)]
    pub owner: Signer<'info>,

    /// Factory state stores the protocol owner address
    #[account(mut)]
    pub factory_state: AccountLoader<'info, FactoryState>,
}
