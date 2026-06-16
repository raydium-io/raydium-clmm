use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseProtocolPosition<'info> {
    #[account(
        mut,
        address = crate::admin::ID @ ErrorCode::NotApproved
    )]
    pub admin: Signer<'info>,

    #[account(
        mut, 
        close = admin
    )]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,
}

pub fn close_protocol_position<'info>(
    _ctx: Context<'info, CloseProtocolPosition<'info>>,
) -> Result<()> {
    Ok(())
}
