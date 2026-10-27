use crate::error::ErrorCode;
use crate::instructions::open_position::open_position;
use crate::instructions::open_position_with_token22_nft::OpenPositionWithToken22Nft;
use crate::util::create_nft_mint_with_extensions;
use crate::util::{parse_remaining_accounts, RemainingAccountsInfo};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::{create, Create};

/// `open_position_with_token22_nft` with a declared account tape.
///
/// There is deliberately no v3 for the Metaplex-metadata position NFT (`open_position_v2`):
/// new positions on hook pools use the Token-2022 NFT, which saves an account and a
/// Metaplex CPI. `open_position_v2` therefore rejects hook mints permanently.
pub fn open_position_v3<'info>(
    ctx: Context<'info, OpenPositionWithToken22Nft<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    with_metadata: bool,
    base_flag: Option<bool>,
    remaining_accounts_info: RemainingAccountsInfo,
) -> Result<()> {
    require!(
        !ctx.accounts.token_account_0.is_frozen() && !ctx.accounts.token_account_1.is_frozen(),
        ErrorCode::NotApproved
    );
    let parsed = parse_remaining_accounts(ctx.remaining_accounts, &remaining_accounts_info)?;

    create_nft_mint_with_extensions(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.personal_position.to_account_info(),
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program_2022,
        with_metadata,
    )?;

    create(CpiContext::new(
        ctx.accounts.associated_token_program.key(),
        Create {
            payer: ctx.accounts.payer.to_account_info(),
            associated_token: ctx.accounts.position_nft_account.to_account_info(),
            authority: ctx.accounts.position_nft_owner.to_account_info(),
            mint: ctx.accounts.position_nft_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program_2022.to_account_info(),
        },
    ))?;

    open_position(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_owner,
        &ctx.accounts.position_nft_mint,
        &ctx.accounts.position_nft_account,
        None,
        &ctx.accounts.pool_state,
        &ctx.accounts.tick_array_lower,
        &ctx.accounts.tick_array_upper,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_account_0.to_account_info(),
        &ctx.accounts.token_account_1.to_account_info(),
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.rent,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &ctx.accounts.associated_token_program,
        None,
        Some(&ctx.accounts.token_program_2022),
        Some(ctx.accounts.vault_0_mint.clone()),
        Some(ctx.accounts.vault_1_mint.clone()),
        parsed.bitmap_extension(),
        parsed.hook_token_0,
        parsed.hook_token_1,
        ctx.bumps.personal_position,
        liquidity,
        amount_0_max,
        amount_1_max,
        tick_lower_index,
        tick_upper_index,
        tick_array_lower_start_index,
        tick_array_upper_start_index,
        with_metadata,
        base_flag,
        true,
    )
}
