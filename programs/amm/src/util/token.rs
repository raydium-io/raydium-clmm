use super::{create_or_allocate_account, get_recent_epoch};
use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::{
    prelude::*,
    solana_program,
    solana_program::program_option::COption,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::memo::spl_memo;
use anchor_spl::token::{self, Token};
use anchor_spl::token_2022::{
    self, get_account_data_size, GetAccountDataSize, InitializeAccount3, InitializeImmutableOwner,
    Token2022,
};
use anchor_spl::token_interface::{initialize_mint2, InitializeMint2, Mint, TokenInterface};
use spl_token_2022::{
    self,
    extension::{
        default_account_state::DefaultAccountState,
        metadata_pointer,
        transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
    state::AccountState,
};
use std::collections::HashSet;

const MINT_WHITELIST: [&'static str; 6] = [
    "HVbpJAQGNpkgBaYBZQBR1t7yFdvaYVp2vCQQfKKEN4tM",
    "Crn4x1Y2HUKko7ox2EZMT6N2t2ZyH7eKtwkBGVnhEq1g",
    "FrBfWJ4qE5sCzKm3k3JaAtqZcXUh4LvJygDeketsrsH4",
    "2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo",
    "DAUcJBg4jSpVoEzASxYzdqHMUN8vuTpQyG2TvDcCHfZg",
    "AUSD1jCcCyPLybk1YnvPWsHQSrZ46dxwoMniN4N2UEB9",
];

pub mod superstate_allowlist {
    use super::{pubkey, Pubkey};
    #[cfg(feature = "devnet")]
    pub const ID: Pubkey = pubkey!("3TRuL3MFvzHaUfQAb6EsSAbQhWdhmYrKxEiViVkdQfXu");
    #[cfg(not(feature = "devnet"))]
    pub const ID: Pubkey = pubkey!("2Yq4T3mPNfjtEyTxSbRjRKqLf1pwbTasuCQrWe6QpM7x");
}

pub fn invoke_memo_instruction<'info>(
    memo_msg: &[u8],
    memo_program: AccountInfo<'info>,
) -> solana_program::entrypoint::ProgramResult {
    let ix = spl_memo::build_memo(memo_msg, &Vec::new());
    let accounts = vec![memo_program];
    solana_program::program::invoke(&ix, &accounts[..])
}

pub fn transfer_from_user_to_pool_vault<'info>(
    signer: &Signer<'info>,
    from: &AccountInfo<'info>,
    to_vault: &AccountInfo<'info>,
    mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    token_program: &AccountInfo<'info>,
    token_program_2022: Option<AccountInfo<'info>>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_token_info = from.to_account_info();
    match (mint, token_program_2022) {
        (Some(mint), Some(token_program_2022)) => {
            if from_token_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }
            token_2022::transfer_checked(
                CpiContext::new(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_token_info,
                        to: to_vault.to_account_info(),
                        authority: signer.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                ),
                amount,
                mint.decimals,
            )
        }
        _ => token::transfer(
            CpiContext::new(
                token_program_info,
                token::Transfer {
                    from: from_token_info,
                    to: to_vault.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            amount,
        ),
    }
}

pub fn transfer_from_pool_vault_to_user<'info>(
    pool_state_loader: &AccountLoader<'info, PoolState>,
    from_vault: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    mint: Option<Box<InterfaceAccount<'info, Mint>>>,
    token_program: &AccountInfo<'info>,
    token_program_2022: Option<AccountInfo<'info>>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut token_program_info = token_program.to_account_info();
    let from_vault_info = from_vault.to_account_info();
    match (mint, token_program_2022) {
        (Some(mint), Some(token_program_2022)) => {
            if from_vault_info.owner == token_program_2022.key {
                token_program_info = token_program_2022.to_account_info()
            }
            token_2022::transfer_checked(
                CpiContext::new_with_signer(
                    token_program_info,
                    token_2022::TransferChecked {
                        from: from_vault_info,
                        to: to.to_account_info(),
                        authority: pool_state_loader.to_account_info(),
                        mint: mint.to_account_info(),
                    },
                    &[&pool_state_loader.load()?.seeds()],
                ),
                amount,
                mint.decimals,
            )
        }
        _ => token::transfer(
            CpiContext::new_with_signer(
                token_program_info,
                token::Transfer {
                    from: from_vault_info,
                    to: to.to_account_info(),
                    authority: pool_state_loader.to_account_info(),
                },
                &[&pool_state_loader.load()?.seeds()],
            ),
            amount,
        ),
    }
}

pub fn close_spl_account<'a, 'b, 'c, 'info>(
    owner: &AccountInfo<'info>,
    destination: &AccountInfo<'info>,
    close_account: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    token_2022::close_account(CpiContext::new_with_signer(
        token_program.to_account_info(),
        token_2022::CloseAccount {
            account: close_account.to_account_info(),
            destination: destination.to_account_info(),
            authority: owner.to_account_info(),
        },
        signers_seeds,
    ))
}

pub fn burn<'a, 'b, 'c, 'info>(
    owner: &Signer<'info>,
    mint: &AccountInfo<'info>,
    burn_account: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    signers_seeds: &[&[&[u8]]],
    amount: u64,
) -> Result<()> {
    let mint_info = mint.to_account_info();
    let token_program_info: AccountInfo<'_> = token_program.to_account_info();
    token_2022::burn(
        CpiContext::new_with_signer(
            token_program_info,
            token_2022::Burn {
                mint: mint_info,
                from: burn_account.to_account_info(),
                authority: owner.to_account_info(),
            },
            signers_seeds,
        ),
        amount,
    )
}

/// Calculate the fee for output amount
pub fn get_transfer_inverse_fee(
    mint_account: Box<InterfaceAccount<Mint>>,
    post_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = get_recent_epoch()?;

        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            let transfer_fee = transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap();
            let transfer_fee_for_check = transfer_fee_config
                .calculate_epoch_fee(epoch, post_fee_amount.checked_add(transfer_fee).unwrap())
                .unwrap();
            if transfer_fee != transfer_fee_for_check {
                return err!(ErrorCode::TransferFeeCalculateNotMatch);
            }
            transfer_fee
        }
    } else {
        0
    };
    Ok(fee)
}

/// Calculate the fee for input amount
pub fn get_transfer_fee(
    mint_account: Box<InterfaceAccount<Mint>>,
    pre_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(get_recent_epoch()?, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}

pub fn support_mint_associated_is_initialized(
    remaining_accounts: &[AccountInfo],
    token_mint: &InterfaceAccount<Mint>,
) -> Result<bool> {
    if remaining_accounts.len() == 0 {
        return Ok(false);
    }
    let (expect_mint_associated, __bump) = Pubkey::find_program_address(
        &[SUPPORT_MINT_SEED.as_bytes(), token_mint.key().as_ref()],
        &crate::id(),
    );
    let mut mint_associated_is_initialized = false;
    for mint_associated_info in remaining_accounts.into_iter() {
        if *mint_associated_info.owner != crate::id()
            || mint_associated_info.key() != expect_mint_associated
        {
            continue;
        }
        let mint_associated = SupportMintAssociated::try_deserialize(
            &mut mint_associated_info.data.borrow().as_ref(),
        )?;
        if mint_associated.mint == token_mint.key() {
            mint_associated_is_initialized = true;
            break;
        }
    }
    return Ok(mint_associated_is_initialized);
}

pub fn is_supported_mint(
    mint_account: &InterfaceAccount<Mint>,
    mint_associated_is_initialized: bool,
) -> Result<bool> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(true);
    }
    let mint_whitelist: HashSet<&str> = MINT_WHITELIST.into_iter().collect();
    if mint_whitelist.contains(mint_account.key().to_string().as_str()) {
        return Ok(true);
    }
    if mint_associated_is_initialized {
        return Ok(true);
    }

    if is_superstate_token(&mint_account) {
        // Supports ScaledUiConfig, which does not work with StateWithExtensions::<spl_token_2022::state::Mint>::unpack
        // To avoid having to resort to other tricks (or upgrading library dependencies), this is simpler.
        return Ok(true);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    let extensions = mint.get_extension_types()?;
    for e in extensions {
        if e != ExtensionType::TransferFeeConfig
            && e != ExtensionType::MetadataPointer
            && e != ExtensionType::TokenMetadata
            && e != ExtensionType::InterestBearingConfig
            && e != ExtensionType::ScaledUiAmount
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn create_position_nft_mint_with_extensions<'info>(
    payer: &Signer<'info>,
    position_nft_mint: &AccountInfo<'info>,
    mint_authority: &AccountInfo<'info>,
    mint_close_authority: &AccountInfo<'info>,
    system_program: &Program<'info, System>,
    token_2022_program: &Program<'info, Token2022>,
    with_matedata: bool,
) -> Result<()> {
    let extensions = if with_matedata {
        [
            ExtensionType::MintCloseAuthority,
            ExtensionType::MetadataPointer,
        ]
        .to_vec()
    } else {
        [ExtensionType::MintCloseAuthority].to_vec()
    };
    let space =
        ExtensionType::try_calculate_account_len::<spl_token_2022::state::Mint>(&extensions)?;

    let lamports = Rent::get()?.minimum_balance(space);

    // create mint account
    create_account(
        CpiContext::new(
            system_program.to_account_info(),
            CreateAccount {
                from: payer.to_account_info(),
                to: position_nft_mint.to_account_info(),
            },
        ),
        lamports,
        space as u64,
        token_2022_program.key,
    )?;

    // initialize token extensions
    for e in extensions {
        match e {
            ExtensionType::MetadataPointer => {
                let ix = metadata_pointer::instruction::initialize(
                    token_2022_program.key,
                    position_nft_mint.key,
                    None,
                    Some(position_nft_mint.key()),
                )?;
                solana_program::program::invoke(
                    &ix,
                    &[
                        token_2022_program.to_account_info(),
                        position_nft_mint.to_account_info(),
                    ],
                )?;
            }
            ExtensionType::MintCloseAuthority => {
                let ix = spl_token_2022::instruction::initialize_mint_close_authority(
                    token_2022_program.key,
                    position_nft_mint.key,
                    Some(mint_close_authority.key),
                )?;
                solana_program::program::invoke(
                    &ix,
                    &[
                        token_2022_program.to_account_info(),
                        position_nft_mint.to_account_info(),
                    ],
                )?;
            }
            _ => {
                return err!(ErrorCode::NotSupportMint);
            }
        }
    }

    // initialize mint account
    initialize_mint2(
        CpiContext::new(
            token_2022_program.to_account_info(),
            InitializeMint2 {
                mint: position_nft_mint.to_account_info(),
            },
        ),
        0,
        &mint_authority.key(),
        None,
    )
}

pub fn create_token_vault_account<'info>(
    payer: &Signer<'info>,
    pool_state: &AccountInfo<'info>,
    token_account: &AccountInfo<'info>,
    token_mint: &InterfaceAccount<'info, Mint>,
    system_program: &Program<'info, System>,
    token_2022_program: &Interface<'info, TokenInterface>,
    signer_seeds: &[&[u8]],
) -> Result<()> {
    let immutable_owner_required = is_superstate_token(token_mint);
    // support both spl_token_program & token_program_2022
    let space = get_account_data_size(
        CpiContext::new(
            token_2022_program.to_account_info(),
            GetAccountDataSize {
                mint: token_mint.to_account_info(),
            },
        ),
        if immutable_owner_required {
            &[anchor_spl::token_2022::spl_token_2022::extension::ExtensionType::ImmutableOwner]
        } else {
            &[]
        },
    )?;

    // create account with or without lamports
    create_or_allocate_account(
        token_2022_program.key,
        payer.to_account_info(),
        system_program.to_account_info(),
        token_account.to_account_info(),
        signer_seeds,
        space.try_into().unwrap(),
    )?;

    // Call initializeImmutableOwner
    if immutable_owner_required {
        token_2022::initialize_immutable_owner(CpiContext::new(
            token_2022_program.to_account_info(),
            InitializeImmutableOwner {
                account: token_account.to_account_info(),
            },
        ))?;
    }

    // Call initializeAccount3
    token_2022::initialize_account3(CpiContext::new(
        token_2022_program.to_account_info(),
        InitializeAccount3 {
            account: token_account.to_account_info(),
            mint: token_mint.to_account_info(),
            authority: pool_state.to_account_info(),
        },
    ))?;

    Ok(())
}

pub fn is_superstate_token(mint_account: &InterfaceAccount<Mint>) -> bool {
    if let COption::Some(freeze_authority) = mint_account.freeze_authority {
        let mint_account_info = mint_account.to_account_info();
        let mint_data = mint_account_info.try_borrow_data().unwrap();
        let mint_state =
            StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data).unwrap();
        let default_account_state_freeze =
            if let Ok(default_account_state) = mint_state.get_extension::<DefaultAccountState>() {
                default_account_state.state == (AccountState::Frozen as u8)
            } else {
                false
            };

        let maybe_permanent_delegate = if let Some(permanent_delegate) =
            spl_token_2022::extension::permanent_delegate::get_permanent_delegate(&mint_state)
        {
            permanent_delegate == superstate_allowlist::ID
        } else {
            false
        };

        superstate_allowlist::ID == freeze_authority
            && *mint_account_info.owner == spl_token_2022::ID
            && default_account_state_freeze
            && maybe_permanent_delegate
    } else {
        false
    }
}
