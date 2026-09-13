use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateRuleset<'info> {
    /// Rulesets are admin-defined
    #[account(mut, address = crate::admin::ID @ ErrorCode::NotApproved)]
    pub owner: Signer<'info>,

    #[account(
        init,
        seeds = [RULESET_SEED.as_bytes(), &index.to_le_bytes()],
        bump,
        payer = owner,
        space = Ruleset::LEN
    )]
    pub ruleset: Account<'info, Ruleset>,

    pub system_program: Program<'info, System>,
}

pub fn create_ruleset(
    ctx: Context<CreateRuleset>,
    index: u16,
    kind: u8,
    flags: u8,
    program_id: Pubkey,
) -> Result<()> {
    super::validate_ruleset_params(kind, flags, &program_id)?;
    let ruleset = &mut ctx.accounts.ruleset;
    ruleset.bump = ctx.bumps.ruleset;
    ruleset.index = index;
    ruleset.kind = kind;
    ruleset.flags = flags;
    ruleset.program_id = program_id;
    Ok(())
}
