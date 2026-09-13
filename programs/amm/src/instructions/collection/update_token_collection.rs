use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateTokenCollection<'info> {
    #[account(address = collection.authority @ ErrorCode::NotApproved)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub collection: Account<'info, TokenCollection>,
}

/// param 0: rebalance_fee_divisor, param 1: new authority (first remaining account)
pub fn update_token_collection(ctx: Context<UpdateTokenCollection>, param: u8, value: u64) -> Result<()> {
    let collection = &mut ctx.accounts.collection;
    match param {
        0 => {
            require_gt!(value, 0, ErrorCode::InvalidUpdateConfigFlag);
            collection.rebalance_fee_divisor = u32::try_from(value).map_err(|_| ErrorCode::InvalidUpdateConfigFlag)?;
        }
        1 => {
            let new_authority = ctx
                .remaining_accounts
                .first()
                .ok_or(ErrorCode::InvalidUpdateConfigFlag)?
                .key();
            collection.authority = new_authority;
        }
        _ => return err!(ErrorCode::InvalidUpdateConfigFlag),
    }
    Ok(())
}
