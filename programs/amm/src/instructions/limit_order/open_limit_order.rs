use crate::libraries::{big_num::U128, fixed_point_64, full_math::MulDiv, tick_math};
use crate::states::*;
use crate::util::get_transfer_fee;
use crate::{error::ErrorCode, Result};
use anchor_lang::{prelude::*, solana_program};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
#[derive(Accounts)]
#[instruction(nonce_index: u8, zero_for_one: bool, tick_index: i32)]
pub struct OpenLimitOrder<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// CHECK:The tick array that contains target tick
    #[account(mut)]
    pub tick_array: UncheckedAccount<'info>,

    /// The order nonce account PDA
    #[account(
        init_if_needed,
        payer = payer,
        seeds = [
            payer.key().as_ref(),
            &[nonce_index as u8],
        ],
        bump,
        space = LimitOrderNonce::LEN,
    )]
    pub limit_order_nonce: Account<'info, LimitOrderNonce>,

    /// The order account PDA
    #[account(
        init,
        payer = payer,
        seeds = [
            payer.key().as_ref(),
            limit_order_nonce.key().as_ref(),
            limit_order_nonce.order_nonce.to_be_bytes().as_ref(),
        ],
        bump,
        space = LimitOrderState::LEN,
    )]
    pub limit_order: Account<'info, LimitOrderState>,

    /// The payer's limit order token account
    #[account(
        mut,
        token::mint = input_vault.mint,
        token::authority = payer,
    )]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The payer's limit order token account for frozen check
    #[account(
        mut,
        token::mint = output_vault.mint,
        token::authority = payer,
    )]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds limit order token
    #[account(
        mut,
        constraint = if zero_for_one {
            input_vault.key() == pool_state.load()?.token_vault_0
        } else {
            input_vault.key() == pool_state.load()?.token_vault_1
        }
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds limit order token for frozen check
    #[account(
        mut,
        constraint = if zero_for_one {
            output_vault.key() == pool_state.load()?.token_vault_1
        } else {
            output_vault.key() == pool_state.load()?.token_vault_0
        }
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint of token vault
    #[account(
        address = input_vault.mint,
    )]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token vault for frozen check
    #[account(
        address = output_vault.mint,
    )]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL-TOKEN / SPL-TOKEN2022 program for input token transfers
    #[account(
        address = *input_vault_mint.to_account_info().owner
    )]
    pub input_token_program: Interface<'info, TokenInterface>,

    /// SPL program for system operations
    pub system_program: Program<'info, System>,
    // remaining accounts:
    //   open_limit_order: [tick array bitmap extension], when the tick needs it
    //   open_limit_order_v2: segments declared by `RemainingAccountsInfo`:
    //     TickArrayBitmapExtension (optional), TransferHookInput
}

pub fn open_limit_order<'info>(
    ctx: Context<'info, OpenLimitOrder<'info>>,
    nonce_index: u8,
    zero_for_one: bool,
    tick_index: i32,
    amount: u64,
) -> Result<()> {
    crate::util::hook::reject_hook_mint(&ctx.accounts.input_vault_mint.to_account_info())?;
    crate::util::hook::reject_hook_mint(&ctx.accounts.output_vault_mint.to_account_info())?;
    let bitmap_extension = ctx.remaining_accounts.first();
    open_limit_order_internal(
        ctx,
        bitmap_extension,
        &[],
        nonce_index,
        zero_for_one,
        tick_index,
        amount,
    )
}

pub fn open_limit_order_internal<'info>(
    ctx: Context<'info, OpenLimitOrder<'info>>,
    bitmap_extension: Option<&'info AccountInfo<'info>>,
    hook_input: &[AccountInfo<'info>],
    nonce_index: u8,
    zero_for_one: bool,
    tick_index: i32,
    amount: u64,
) -> Result<()> {
    require!(
        !ctx.accounts.input_token_account.is_frozen()
            && !ctx.accounts.output_token_account.is_frozen(),
        ErrorCode::NotApproved
    );

    let (tick_spacing, tick_current) = {
        let pool_state = ctx.accounts.pool_state.load()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::LimitOrder)
            || !pool_state.get_status_by_bit(PoolStatusBitIndex::Swap)
        {
            return err!(ErrorCode::NotApproved);
        }
        (pool_state.tick_spacing, pool_state.tick_current)
    };
    check_tick_index(tick_index, zero_for_one, tick_current, tick_spacing)?;
    let transfer_fee = get_transfer_fee(ctx.accounts.input_vault_mint.clone(), amount)?;
    let amount_without_transfer_fee = amount
        .checked_sub(transfer_fee)
        .ok_or(ErrorCode::CalculateOverflow)?;

    check_limit_order_amount(amount_without_transfer_fee, tick_index, zero_for_one)?;

    let tick_array_start_index = TickArrayState::get_array_start_index(tick_index, tick_spacing);
    let tick_array_key = Pubkey::find_program_address(
        &[
            TICK_ARRAY_SEED.as_bytes(),
            ctx.accounts.pool_state.key().as_ref(),
            &tick_array_start_index.to_be_bytes(),
        ],
        &crate::id(),
    )
    .0;
    require_keys_eq!(tick_array_key, ctx.accounts.tick_array.key());
    let tick_array_loader = TickArrayState::get_or_create_tick_array(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.tick_array.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.pool_state.key(),
        tick_array_start_index,
        tick_spacing,
    )?;
    let mut tick_array = tick_array_loader.load_mut()?;
    let tick_array_start_index = tick_array.start_tick_index;
    let before_init_tick_count = tick_array.initialized_tick_count;

    let tick_state = tick_array.get_tick_state_mut(tick_index, tick_spacing)?;
    require!(
        tick_state.order_phase < u64::MAX,
        ErrorCode::OrderPhaseSaturated
    );
    let tick_initialized_before = tick_state.is_initialized();
    tick_state.tick = tick_index;

    let order = &mut ctx.accounts.limit_order;
    order.initialize(
        ctx.accounts.pool_state.key(),
        ctx.accounts.payer.key(),
        tick_index,
        zero_for_one,
        amount_without_transfer_fee,
        tick_state.order_phase,
        solana_program::clock::Clock::get()?.unix_timestamp as u64,
    );
    // place order, amount always be added to orders_amount
    tick_state.orders_amount = tick_state
        .orders_amount
        .checked_add(amount_without_transfer_fee)
        .ok_or(ErrorCode::CalculateOverflow)?;
    if !tick_initialized_before {
        // This is very important,
        // otherwise, the tick_array may not be correctly found during swap operations
        tick_array.update_initialized_tick_count(true)?;
        if before_init_tick_count == 0 {
            let mut pool_state = ctx.accounts.pool_state.load_mut()?;
            let use_tickarray_bitmap_extension =
                pool_state.is_overflow_default_tickarray_bitmap(vec![tick_array_start_index]);

            let tickarray_bitmap_extension = crate::instructions::resolve_bitmap_extension(
                use_tickarray_bitmap_extension,
                bitmap_extension,
                ctx.accounts.pool_state.key(),
            )?;

            pool_state.flip_tick_array_bit(tickarray_bitmap_extension, tick_array_start_index)?;
        }
    }
    // A hook whose extras name this tick array would fail the CPI's borrow check.
    drop(tick_array);

    let limit_order_nonce = &mut ctx.accounts.limit_order_nonce;
    if limit_order_nonce.user_wallet == Pubkey::default() {
        limit_order_nonce.user_wallet = ctx.accounts.payer.key();
        limit_order_nonce.nonce_index = nonce_index;
    }
    limit_order_nonce.increase_order_nonce()?;

    crate::util::transfer_checked_with_hook(
        &ctx.accounts.input_token_program.to_account_info(),
        &ctx.accounts.input_token_account.to_account_info(),
        &ctx.accounts.input_vault_mint.to_account_info(),
        &ctx.accounts.input_vault.to_account_info(),
        &ctx.accounts.payer.to_account_info(),
        hook_input,
        amount,
        ctx.accounts.input_vault_mint.decimals,
        &[],
    )?;

    emit!(OpenLimitOrderEvent {
        pool_id: ctx.accounts.pool_state.key(),
        limit_order: ctx.accounts.limit_order.key(),
        zero_for_one: ctx.accounts.limit_order.zero_for_one,
        tick_index: ctx.accounts.limit_order.tick_index,
        total_amount: ctx.accounts.limit_order.total_amount,
        transfer_fee: transfer_fee,
    });

    Ok(())
}

/// Validates limit order amount: must be non-zero and the implied output amount
/// (at this tick's price) must be in [1, u64::MAX - 2].
pub fn check_limit_order_amount(amount: u64, tick_index: i32, zero_for_one: bool) -> Result<()> {
    if amount == 0 {
        return err!(ErrorCode::ZeroAmountSpecified);
    }
    // amount_out: zero_for_one => floor(amount * price_x64 / Q64); !zero_for_one => floor(amount * Q64 / price_x64).
    // Price is floor/ceil(sqrt_price²/Q64). Two floors ⇒ error < 2 (output-token base units).
    let amount_out = if zero_for_one {
        let token_0_price_x64: U128 = tick_math::get_price_at_tick(tick_index, false)?;
        U128::from(amount)
            .mul_div_floor(token_0_price_x64, U128::from(fixed_point_64::Q64))
            .ok_or(ErrorCode::CalculateOverflow)?
    } else {
        let token_0_price_x64 = tick_math::get_price_at_tick(tick_index, true)?;
        U128::from(amount)
            .mul_div_floor(U128::from(fixed_point_64::Q64), token_0_price_x64)
            .ok_or(ErrorCode::CalculateOverflow)?
    };
    if amount_out < U128::one() || amount_out >= U128::from(u64::MAX - 2) {
        return err!(ErrorCode::InvalidLimitOrderAmount);
    }
    Ok(())
}

pub fn check_tick_index(
    tick_index: i32,
    zero_for_one: bool,
    tick_current: i32,
    tick_spacing: u16,
) -> Result<()> {
    require!(
        tick_index > tick_math::MIN_TICK && tick_index < tick_math::MAX_TICK,
        ErrorCode::InvalidTickIndex
    );

    if tick_index % i32::from(tick_spacing) != 0 {
        return err!(ErrorCode::TickAndSpacingNotMatch);
    }

    if (zero_for_one && tick_index <= tick_current) || (!zero_for_one && tick_index >= tick_current)
    {
        return err!(ErrorCode::InvalidTickIndex);
    }
    Ok(())
}
