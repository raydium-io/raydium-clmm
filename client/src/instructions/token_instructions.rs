use anchor_client::{Client, Cluster};
use anyhow::Result;
use solana_sdk::{
    program_pack::Pack,
    signature::{Keypair, Signer},
    pubkey::Pubkey,
    system_instruction,
    instruction::Instruction,
};
use std::rc::Rc;
use super::super::{ClientConfig, read_keypair_file};

pub fn create_and_init_mint_instr(
    config: &ClientConfig,
    mint_key: &Pubkey,
    mint_authority: &Pubkey,
    decimals: u8,
) ->  Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_token::id());

    let instructions = program
        .request()
        .instruction(
            system_instruction::create_account(
                &program.payer(),
                mint_key,
                program.rpc().get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?,
                spl_token::state::Mint::LEN as u64,
                &program.id(),
            )
        )
        .instruction(
            spl_token::instruction::initialize_mint(
                &program.id(),
                mint_key,
                mint_authority,
                None,
                decimals,
            )?
        )
        .instructions()?;
    Ok(instructions)
}

pub fn create_account_rent_exmpt_instr(
    config: &ClientConfig,
    new_account_key: &Pubkey,
    owner: Pubkey,
    data_size: usize,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(owner);
    let instructions = program
        .request()
        .instruction(
            system_instruction::create_account(
                &program.payer(),
                &new_account_key,
                program.rpc().get_minimum_balance_for_rent_exemption(data_size)?,
                data_size as u64,
                &program.id(),
            )
        )
        .instructions()?;
    Ok(instructions)
}

pub fn create_ata_token_account_instr(
    config: &ClientConfig,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_token::id());
    let instructions = program
            .request()
            .instruction(
                spl_associated_token_account::create_associated_token_account(
                    &program.payer(),
                    owner,
                    mint
                )
            )
            .instructions()?;
    Ok(instructions)
}

pub fn create_and_init_spl_token(
    config: &ClientConfig,
    new_account_key: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_associated_token_account::id());

    let instructions = program
        .request()
        .instruction(
            system_instruction::create_account(
                &program.payer(),
                &mint,
                program.rpc().get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN)?,
                spl_token::state::Account::LEN as u64,
                &program.id(),
            )
        )
        .instruction(
            spl_token::instruction::initialize_account(
                &program.id(),
                new_account_key,
                mint,
                owner,
            )?
        )
        .instructions()?;
    Ok(instructions)
}

pub fn close_token_account(
    config: &ClientConfig,
    close_account: &Pubkey,
    destination: &Pubkey,
    owner: &Keypair,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_token::id());
    let instructions = program
        .request()
        .instruction(
            spl_token::instruction::close_account(
                &program.id(),
                close_account,
                destination,
                &owner.pubkey(),
                &[]
            )?
        )
        .signer(owner)
        .instructions()?;
    Ok(instructions)
}

pub fn spl_token_transfer_instr(
    config: &ClientConfig,
    from: &Pubkey,
    to: &Pubkey,
    amount: u64,
    from_authority: &Keypair,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_token::id());
    let instructions = program
        .request()
        .instruction(
            spl_token::instruction::transfer(
                &program.id(),
                from,
                to,
                &from_authority.pubkey(),
                &[],
                amount
            )?
        )
        .signer(from_authority)
        .instructions()?;
    Ok(instructions)
}

pub fn spl_token_mint_to_instr(
    config: &ClientConfig,
    mint: &Pubkey,
    to: &Pubkey,
    amount: u64,
    mint_authority: &Keypair,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(spl_token::id());
    let instructions = program
        .request()
        .instruction(
            spl_token::instruction::mint_to(
                &program.id(),
                mint,
                to,
                &mint_authority.pubkey(),
                &[],
                amount
            )?
        )
        .signer(mint_authority)
        .instructions()?;
    Ok(instructions)
}