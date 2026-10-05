//! Mint extension initialization parameters.
//!
//! Vendored from `spl-token-client`, which this crate used for this one enum and nothing else.
//! That dependency pulled its own `spl-token-2022` and `solana-*` family: 0.19 brings the agave
//! 4.x crates, which land `solana-transaction` 3 and 4 in the same graph, and 0.18 brings
//! `spl-token-2022` 10, which cannot coexist with the 11 this crate already uses. Both make the
//! workspace fail to build. The enum itself is a thin wrapper over instruction builders we
//! already have.
//!
//! Only the variants the CLI constructs are kept. Add more from upstream as needed.

use anyhow::Result;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey};
use solana_zk_sdk_pod::encryption::elgamal::PodElGamalPubkey;
use spl_token_2022::{
    extension::{
        confidential_transfer, default_account_state, interest_bearing_mint, transfer_fee,
        ExtensionType,
    },
    instruction,
    state::AccountState,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionInitializationParams {
    ConfidentialTransferMint {
        authority: Option<Pubkey>,
        auto_approve_new_accounts: bool,
        auditor_elgamal_pubkey: Option<PodElGamalPubkey>,
    },
    DefaultAccountState {
        state: AccountState,
    },
    MintCloseAuthority {
        close_authority: Option<Pubkey>,
    },
    TransferFeeConfig {
        transfer_fee_config_authority: Option<Pubkey>,
        withdraw_withheld_authority: Option<Pubkey>,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    },
    InterestBearingConfig {
        rate_authority: Option<Pubkey>,
        rate: i16,
    },
    NonTransferable,
    PermanentDelegate {
        delegate: Pubkey,
    },
}

impl ExtensionInitializationParams {
    /// The extension this parameter set initializes, for sizing the mint account.
    pub fn extension(&self) -> ExtensionType {
        match self {
            Self::ConfidentialTransferMint { .. } => ExtensionType::ConfidentialTransferMint,
            Self::DefaultAccountState { .. } => ExtensionType::DefaultAccountState,
            Self::MintCloseAuthority { .. } => ExtensionType::MintCloseAuthority,
            Self::TransferFeeConfig { .. } => ExtensionType::TransferFeeConfig,
            Self::InterestBearingConfig { .. } => ExtensionType::InterestBearingConfig,
            Self::NonTransferable => ExtensionType::NonTransferable,
            Self::PermanentDelegate { .. } => ExtensionType::PermanentDelegate,
        }
    }

    /// The initialization instruction, which must run before `initialize_mint`.
    pub fn instruction(
        self,
        token_program_id: &Pubkey,
        mint: &Pubkey,
    ) -> Result<Instruction, solana_sdk::program_error::ProgramError> {
        match self {
            Self::ConfidentialTransferMint {
                authority,
                auto_approve_new_accounts,
                auditor_elgamal_pubkey,
            } => confidential_transfer::instruction::initialize_mint(
                token_program_id,
                mint,
                authority,
                auto_approve_new_accounts,
                auditor_elgamal_pubkey,
            ),
            Self::DefaultAccountState { state } => {
                default_account_state::instruction::initialize_default_account_state(
                    token_program_id,
                    mint,
                    &state,
                )
            }
            Self::MintCloseAuthority { close_authority } => {
                instruction::initialize_mint_close_authority(
                    token_program_id,
                    mint,
                    close_authority.as_ref(),
                )
            }
            Self::TransferFeeConfig {
                transfer_fee_config_authority,
                withdraw_withheld_authority,
                transfer_fee_basis_points,
                maximum_fee,
            } => transfer_fee::instruction::initialize_transfer_fee_config(
                token_program_id,
                mint,
                transfer_fee_config_authority.as_ref(),
                withdraw_withheld_authority.as_ref(),
                transfer_fee_basis_points,
                maximum_fee,
            ),
            Self::InterestBearingConfig {
                rate_authority,
                rate,
            } => interest_bearing_mint::instruction::initialize(
                token_program_id,
                mint,
                rate_authority,
                rate,
            ),
            Self::NonTransferable => {
                instruction::initialize_non_transferable_mint(token_program_id, mint)
            }
            Self::PermanentDelegate { delegate } => {
                instruction::initialize_permanent_delegate(token_program_id, mint, &delegate)
            }
        }
    }
}
