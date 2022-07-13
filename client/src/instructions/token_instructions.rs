use anchor_client::Client;
use anyhow::Result;
use solana_sdk::{
    program_pack::Pack,
    signature::{Keypair, Signer, Signature},
    pubkey::Pubkey,
    system_instruction
};

use rand::rngs::OsRng;

pub fn create_and_init_mint(
    client: &Client,
    mint_authority: &Pubkey,
    decimals: u8,
) ->  Result<(Keypair, Signature)> {
    let program = client.program(spl_token::id());
    let mint = Keypair::generate(&mut OsRng);

    let signature = program
        .request()
        .instruction(
            system_instruction::create_account(
                &program.payer(),
                &mint.pubkey(),
                program.rpc().get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?,
                spl_token::state::Mint::LEN as u64,
                &program.id(),
            )
        )
        .instruction(
            spl_token::instruction::initialize_mint(
                &program.id(),
                &mint.pubkey(),
                mint_authority,
                None,
                decimals,
            )?
        )
        .signer(&mint)
        .send()?;
    Ok((mint, signature))
}

pub fn create_account_rent_exmpt(
    client: &Client,
    owner: Pubkey,
    data_size: usize,
) -> Result<Signature> {
    let new_account = Keypair::generate(&mut OsRng);
    let program = client.program(owner);
    let signature = program
        .request()
        .instruction(
            system_instruction::create_account(
                &program.payer(),
                &new_account.pubkey(),
                program.rpc().get_minimum_balance_for_rent_exemption(data_size)?,
                data_size as u64,
                &program.id(),
            )
        )
        .signer(&new_account)
        .send()?;
    Ok(signature)
}

pub fn create_ata_token_account(
    client: &Client,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Signature> {
    let program = client.program(spl_token::id());
    let signature = program
            .request()
            .instruction(
                spl_associated_token_account::create_associated_token_account(
                    &program.payer(),
                    owner,
                    mint
                )
            )
            .send()?;
    Ok(signature)
}

pub fn create_and_init_spl_token(
    client: &Client,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<Signature> {
    let new_account = Keypair::generate(&mut OsRng);
    let program = client.program(spl_associated_token_account::id());

    let signature = program
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
                &new_account.pubkey(),
                mint,
                owner,
            )?
        )
        .signer(&new_account)
        .send()?;
    Ok(signature)
}

pub fn close_token_account(
    client: &Client,
    close_account: &Pubkey,
    destination: &Pubkey,
    owner: &Keypair,
) -> Result<Signature> {
    let program = client.program(spl_token::id());
    let signature = program
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
        .send()?;
    Ok(signature)
}

pub fn spl_token_transfer(
    client: &Client,
    from: &Pubkey,
    to: &Pubkey,
    amount: u64,
    from_authority: &Keypair,
) -> Result<Signature> {
    let program = client.program(spl_token::id());
    let signature = program
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
        .send()?;
    Ok(signature)
}

pub fn spl_token_mint_to(
    client: &Client,
    mint: &Pubkey,
    to: &Pubkey,
    amount: u64,
    mint_authority: &Keypair,
) -> Result<Signature> {
    let program = client.program(spl_token::id());
    let signature = program
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
        .send()?;
    Ok(signature)
}