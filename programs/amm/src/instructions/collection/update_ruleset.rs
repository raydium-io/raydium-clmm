use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateRuleset<'info> {
    #[account(address = crate::admin::ID @ ErrorCode::NotApproved)]
    pub owner: Signer<'info>,

    #[account(mut)]
    pub ruleset: Account<'info, Ruleset>,
}

pub fn update_ruleset(ctx: Context<UpdateRuleset>, kind: u8, flags: u8, program_id: Pubkey) -> Result<()> {
    super::validate_ruleset_params(kind, flags, &program_id)?;
    let ruleset = &mut ctx.accounts.ruleset;
    ruleset.kind = kind;
    ruleset.flags = flags;
    ruleset.program_id = program_id;
    Ok(())
}
