use anchor_client::Client;
use anchor_lang::prelude::AccountMeta;
use anyhow::Result;
use solana_sdk::{
    system_program,
    sysvar,
    signature::{Signature, Keypair, Signer},
    pubkey::Pubkey,
};
use mpl_token_metadata::{state::PREFIX as MPL_PREFIX};

use raydium_amm_v3::instruction as raydium_instruction;
use raydium_amm_v3::accounts as raydium_accounts;
use raydium_amm_v3::{
    states::{AMM_CONFIG_SEED, FEE_SEED, POOL_SEED, POOL_VAULT_SEED, OBSERVATION_SEED, TICK_SEED, BITMAP_SEED, POSITION_SEED},
};

pub fn create_amm_config_tx(
    client: &Client,
    raydium_program: &Pubkey,
    admin: &Keypair,
    protocol_fee_rate: u32
) -> Result<Signature> {
    let program = client.program(*raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::CreateAmmConfig{
                owner: admin.pubkey(),
                amm_config: amm_config_key,
                system_program: system_program::id(),
            }
        )
        .args(raydium_instruction::CreateAmmConfig{protocol_fee_rate})
        .signer(admin)
        .send()?;
    Ok(signature)
}

pub fn set_new_config_owner_tx(
    client: &Client,
    raydium_program: &Pubkey,
    admin: &Keypair,
    new_owner: &Pubkey,
) -> Result<Signature> {
    let program = client.program(*raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::SetNewOwner{
                owner: admin.pubkey(),
                new_owner: *new_owner,
                amm_config: amm_config_key,
            }
        )
        .signer(admin)
        .send()?;
    Ok(signature)
}

pub fn set_protocol_fee_rate_tx(
    client: &Client,
    raydium_program: &Pubkey,
    admin: &Keypair,
    protocol_fee_rate: u32,
) -> Result<Signature> {
    let program = client.program(*raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::SetProtocolFeeRate{
                owner: admin.pubkey(),
                amm_config: amm_config_key,
            }
        )
        .args(raydium_instruction::SetProtocolFeeRate{protocol_fee_rate})
        .signer(admin)
        .send()?;
    Ok(signature)
}

pub fn create_fee_account_tx(
    client: &Client,
    raydium_program: &Pubkey,
    admin: &Keypair,
    fee: u32,
    tick_spacing: u16,
) -> Result<Signature> {
    let program = client.program(*raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let (fee_account_key, __bump) = Pubkey::find_program_address(&[FEE_SEED.as_bytes(), &fee.to_be_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::CreateFeeAccount{
                owner: admin.pubkey(),
                amm_config: amm_config_key,
                fee_state: fee_account_key,
                system_program: system_program::id(),
            }
        )
        .args(raydium_instruction::CreateFeeAccount{fee, tick_spacing})
        .signer(admin)
        .send()?;
    Ok(signature)
}

pub fn create_pool_tx(
    client: &Client,
    raydium_program: Pubkey,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    sqrt_price_x64: u128,
    fee: u32,
) -> Result<Signature> {
    let program = client.program(raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let (pool_account_key, __bump) = Pubkey::find_program_address(&[POOL_SEED.as_bytes(), amm_config_key.to_bytes().as_ref(), token_mint_0.to_bytes().as_ref(), token_mint_1.to_bytes().as_ref(), &fee.to_be_bytes()], &program.id());
    let (token_vault_0, __bump) = Pubkey::find_program_address(&[POOL_VAULT_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), token_mint_0.to_bytes().as_ref()], &program.id());
    let (token_vault_1, __bump) = Pubkey::find_program_address(&[POOL_VAULT_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), token_mint_1.to_bytes().as_ref()], &program.id());
    let (fee_account_key, __bump) = Pubkey::find_program_address(&[FEE_SEED.as_bytes(), &fee.to_be_bytes()], &program.id());
    let (observation_account_key, __bump) = Pubkey::find_program_address(&[OBSERVATION_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &0_u16.to_be_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::CreatePool{
                pool_creator: program.payer(),
                amm_config: amm_config_key,
                pool_state: pool_account_key,
                token_mint_0,
                token_mint_1,
                token_vault_0,
                token_vault_1,
                fee_state: fee_account_key,
                initial_first_observation: observation_account_key,
                token_program: spl_token::id(),
                system_program: system_program::id(),
                rent: sysvar::rent::id(),
            }
        )
        .args(raydium_instruction::CreatePool{sqrt_price_x64})
        .send()?;
    Ok(signature)
}

pub fn open_position_tx(
    client: &Client,
    raydium_program: Pubkey,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    last_observation_account: Pubkey,
    next_observation_account: Pubkey,
    nft_mint_key: Pubkey,
    nft_to_owner: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    word_lower_index: i16,
    word_upper_index: i16,
) -> Result<Signature> {
    let program = client.program(raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let nft_ata_token_account = spl_associated_token_account::get_associated_token_address(&program.payer(), &nft_mint_key);
    let (metadata_account_key, _bump) = Pubkey::find_program_address(&[MPL_PREFIX.as_bytes(), mpl_token_metadata::id().to_bytes().as_ref(), nft_mint_key.to_bytes().as_ref()], &mpl_token_metadata::id());
    let (protocol_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes(), &tick_upper_index.to_be_bytes()], &program.id());
    let (tick_lower_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes()], &program.id());
    let (tick_upper_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_upper_index.to_be_bytes()], &program.id());
    let (bitmap_lower_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_lower_index.to_be_bytes()], &program.id());
    let (bitmap_upper_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_upper_index.to_be_bytes()], &program.id());
    let (personal_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::OpenPosition{
                payer: program.payer(),
                position_nft_owner: nft_to_owner,
                amm_config: amm_config_key,
                position_nft_mint: nft_mint_key,
                position_nft_account: nft_ata_token_account,
                metadata_account: metadata_account_key,
                pool_state: pool_account_key,
                protocol_position: protocol_position_key,
                tick_lower: tick_lower_key,
                tick_upper: tick_upper_key,
                tick_bitmap_lower: bitmap_lower_key,
                tick_bitmap_upper: bitmap_upper_key,
                personal_position: personal_position_key,
                token_account_0: user_token_account_0,
                token_account_1: user_token_account_1,
                token_vault_0: token_vault_0,
                token_vault_1: token_vault_1,
                last_observation: last_observation_account,
                next_observation: next_observation_account,
                rent: sysvar::rent::id(),
                system_program: system_program::id(),
                token_program: spl_token::id(),
                associated_token_program: spl_associated_token_account::id(),
                metadata_program: mpl_token_metadata::id(),
            }
        )
        .args(raydium_instruction::OpenPosition{amount_0_desired, amount_1_desired, amount_0_min, amount_1_min, tick_lower_index, tick_upper_index, word_lower_index, word_upper_index})
        .send()?;
    Ok(signature)
}

pub fn increase_liquidity_tx(
    client: &Client,
    raydium_program: Pubkey,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    last_observation_account: Pubkey,
    next_observation_account: Pubkey,
    nft_mint_key: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    amount_0_desired: u64,
    amount_1_desired: u64,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    word_lower_index: i16,
    word_upper_index: i16,
) -> Result<Signature> {
    let program = client.program(raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let nft_ata_token_account = spl_associated_token_account::get_associated_token_address(&program.payer(), &nft_mint_key);
    let (tick_lower_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes()], &program.id());
    let (tick_upper_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_upper_index.to_be_bytes()], &program.id());
    let (bitmap_lower_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_lower_index.to_be_bytes()], &program.id());
    let (bitmap_upper_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_upper_index.to_be_bytes()], &program.id());
    let (protocol_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes(), &tick_upper_index.to_be_bytes()], &program.id());
    let (personal_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()], &program.id());

    let signature = program
        .request()
        .accounts(
            raydium_accounts::IncreaseLiquidity{
                nft_owner: program.payer(),
                amm_config: amm_config_key,
                nft_account: nft_ata_token_account,
                pool_state: pool_account_key,
                protocol_position: protocol_position_key,
                personal_position: personal_position_key,
                tick_lower: tick_lower_key,
                tick_upper: tick_upper_key,
                tick_bitmap_lower: bitmap_lower_key,
                tick_bitmap_upper: bitmap_upper_key,
                token_account_0: user_token_account_0,
                token_account_1: user_token_account_1,
                token_vault_0,
                token_vault_1,
                last_observation: last_observation_account,
                next_observation: next_observation_account,
                token_program: spl_token::id(),
            }
        )
        .args(raydium_instruction::IncreaseLiquidity{amount_0_desired, amount_1_desired, amount_0_min, amount_1_min})
        .send()?;
    Ok(signature)
}

pub fn decrease_liquidity_tx(
    client: &Client,
    raydium_program: Pubkey,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    last_observation_account: Pubkey,
    next_observation_account: Pubkey,
    nft_mint_key: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    word_lower_index: i16,
    word_upper_index: i16,
) -> Result<Signature> {
    let program = client.program(raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let nft_ata_token_account = spl_associated_token_account::get_associated_token_address(&program.payer(), &nft_mint_key);
    let (personal_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()], &program.id());
    let (protocol_position_key, __bump) = Pubkey::find_program_address(&[POSITION_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes(), &tick_upper_index.to_be_bytes()], &program.id());
    let (tick_lower_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_lower_index.to_be_bytes()], &program.id());
    let (tick_upper_key, __bump) = Pubkey::find_program_address(&[TICK_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &tick_upper_index.to_be_bytes()], &program.id());
    let (bitmap_lower_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_lower_index.to_be_bytes()], &program.id());
    let (bitmap_upper_key, __bump) = Pubkey::find_program_address(&[BITMAP_SEED.as_bytes(), pool_account_key.to_bytes().as_ref(), &word_upper_index.to_be_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::DecreaseLiquidity{
                nft_owner: program.payer(),
                nft_account: nft_ata_token_account,
                personal_position: personal_position_key,
                amm_config: amm_config_key,
                pool_state: pool_account_key,
                protocol_position: protocol_position_key,
                token_vault_0,
                token_vault_1,
                tick_lower: tick_lower_key,
                tick_upper: tick_upper_key,
                tick_bitmap_lower: bitmap_lower_key,
                tick_bitmap_upper: bitmap_upper_key,
                last_observation: last_observation_account,
                next_observation: next_observation_account,
                recipient_token_account_0: user_token_account_0,
                recipient_token_account_1: user_token_account_1,
                token_program: spl_token::id(),
            }
        )
        .args(raydium_instruction::DecreaseLiquidity{liquidity, amount_0_min, amount_1_min})
        .send()?;
    Ok(signature)
}

pub fn swap_tx(
    client: &Client,
    raydium_program: Pubkey,
    pool_account_key: Pubkey,
    input_vault: Pubkey,
    output_vault: Pubkey,
    last_observation_account: Pubkey,
    next_observation_account: Pubkey,
    user_input_token: Pubkey,
    user_out_put_token: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<Signature> {
    let program = client.program(raydium_program);
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[AMM_CONFIG_SEED.as_bytes()], &program.id());
    let signature = program
        .request()
        .accounts(
            raydium_accounts::SwapSingle{
                payer: program.payer(),
                amm_config: amm_config_key,
                pool_state: pool_account_key,
                input_token_account: user_input_token,
                output_token_account: user_out_put_token,
                input_vault,
                output_vault,
                last_observation: last_observation_account,
                next_observation: next_observation_account,
                token_program: spl_token::id(),
            }
        )
        .accounts(remaining_accounts)
        .args(raydium_instruction::Swap{amount, other_amount_threshold, sqrt_price_limit_x64, is_base_input})
        .send()?;
    Ok(signature)
}
