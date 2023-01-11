use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateOperationAccount<'info> {
    /// Address to be set as operation account owner.
    #[account(
        address = crate::admin::id() @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    /// Initialize operation state account to store operation owner address and white list mint.
    #[account(
        mut,
        seeds = [
            OPERATION_SEED.as_bytes(),
        ],
        bump,
    )]
    pub operation_state: AccountLoader<'info, OperationState>,

    pub system_program: Program<'info, System>,
}

pub fn update_operation_account(
    ctx: Context<UpdateOperationAccount>,
    param: u8,
    keys: Vec<Pubkey>,
) -> Result<()> {
    let mut operation_state = ctx.accounts.operation_state.load_mut()?;
    let match_param = Some(param);
    match match_param {
        Some(0) => operation_state.update_operation_owner(keys),
        Some(1) => operation_state.remove_operation_owner(keys),
        Some(2) => operation_state.update_whitelist_mint(keys),
        Some(3) => operation_state.remove_whitelist_mint(keys),
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }
    Ok(())
}
