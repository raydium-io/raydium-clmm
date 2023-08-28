use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreateTickArrayBitmapExtension<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Initialize an account to store the pool state
    #[account()]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Initialize an account to store if a tick array is initialized.
    #[account(
        init,
        seeds = [
            POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = payer,
        space = TickArrayBitmapExtension::LEN
    )]
    pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,

    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_tick_array_bitmap_extension(
    ctx: Context<CreateTickArrayBitmapExtension>,
) -> Result<()> {
    let mut tick_array_bitmap_extension = ctx.accounts.tick_array_bitmap.load_init()?;
    tick_array_bitmap_extension.initialize(ctx.accounts.pool_state.key());
    Ok(())
}
