#![allow(dead_code)]
use anchor_client::{Client, Cluster};
use anyhow::{format_err, Result};
use solana_account_decoder::{
    parse_token::{TokenAccountType, UiAccountState},
    UiAccountData,
};
use solana_client::{rpc_client::RpcClient, rpc_request::TokenAccountsFilter};
use solana_sdk::{
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

use arrayref::array_refs;
use configparser::ini::Ini;
use rand::rngs::OsRng;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

mod instructions;
use instructions::amm_instructions::*;
use instructions::rpc::*;
use instructions::token_instructions::*;
use instructions::utils::*;
use raydium_amm_v3::libraries::{fixed_point_64, tick_math};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    http_url: String,
    ws_url: String,
    payer_path: String,
    admin_path: String,
    raydium_v3_program: Pubkey,
    amm_config_key: Pubkey,

    mint0: Option<Pubkey>,
    mint1: Option<Pubkey>,
    pool_id_account: Option<Pubkey>,
    amm_config_index: u16,
}

fn load_cfg(client_config: &String) -> Result<ClientConfig> {
    let mut config = Ini::new();
    let _map = config.load(client_config).unwrap();
    let http_url = config.get("Global", "http_url").unwrap();
    if http_url.is_empty() {
        panic!("http_url must not be empty");
    }
    let ws_url = config.get("Global", "ws_url").unwrap();
    if ws_url.is_empty() {
        panic!("ws_url must not be empty");
    }
    let payer_path = config.get("Global", "payer_path").unwrap();
    if payer_path.is_empty() {
        panic!("payer_path must not be empty");
    }
    let admin_path = config.get("Global", "admin_path").unwrap();
    if admin_path.is_empty() {
        panic!("admin_path must not be empty");
    }

    let raydium_v3_program_str = config.get("Global", "raydium_v3_program").unwrap();
    if raydium_v3_program_str.is_empty() {
        panic!("raydium_v3_program must not be empty");
    }
    let raydium_v3_program = Pubkey::from_str(&raydium_v3_program_str).unwrap();

    let mut mint0 = None;
    let mint0_str = config.get("Pool", "mint0").unwrap();
    if !mint0_str.is_empty() {
        mint0 = Some(Pubkey::from_str(&mint0_str).unwrap());
    }
    let mut mint1 = None;
    let mint1_str = config.get("Pool", "mint1").unwrap();
    if !mint1_str.is_empty() {
        mint1 = Some(Pubkey::from_str(&mint1_str).unwrap());
    }
    let amm_config_index = config.getuint("Pool", "amm_config_index").unwrap().unwrap() as u16;

    let (amm_config_key, __bump) = Pubkey::find_program_address(
        &[
            raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
            &amm_config_index.to_be_bytes(),
        ],
        &raydium_v3_program,
    );

    let pool_id_account = if mint0 != None && mint1 != None {
        if mint0.unwrap() > mint1.unwrap() {
            let temp_mint = mint0;
            mint0 = mint1;
            mint1 = temp_mint;
        }
        Some(
            Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::POOL_SEED.as_bytes(),
                    amm_config_key.to_bytes().as_ref(),
                    mint0.unwrap().to_bytes().as_ref(),
                    mint1.unwrap().to_bytes().as_ref(),
                ],
                &raydium_v3_program,
            )
            .0,
        )
    } else {
        None
    };

    Ok(ClientConfig {
        http_url,
        ws_url,
        payer_path,
        admin_path,
        raydium_v3_program,
        amm_config_key,
        mint0,
        mint1,
        pool_id_account,
        amm_config_index,
    })
}
fn read_keypair_file(s: &str) -> Result<Keypair> {
    solana_sdk::signature::read_keypair_file(s)
        .map_err(|_| format_err!("failed to read keypair from {}", s))
}
fn write_keypair_file(keypair: &Keypair, outfile: &str) -> Result<String> {
    solana_sdk::signature::write_keypair_file(keypair, outfile)
        .map_err(|_| format_err!("failed to write keypair to {}", outfile))
}
fn path_is_exist(path: &str) -> bool {
    Path::new(path).exists()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TokenInfo {
    key: Pubkey,
    mint: Pubkey,
    amount: u64,
    decimals: u8,
}
fn get_nft_account_and_position_by_owner(
    client: &RpcClient,
    owner: &Pubkey,
    raydium_amm_v3_program: &Pubkey,
) -> (Vec<TokenInfo>, Vec<Pubkey>) {
    let all_tokens = client
        .get_token_accounts_by_owner(owner, TokenAccountsFilter::ProgramId(spl_token::id()))
        .unwrap();
    let mut nft_account = Vec::new();
    let mut user_position_account = Vec::new();
    for keyed_account in all_tokens {
        if let UiAccountData::Json(parsed_account) = keyed_account.account.data {
            if parsed_account.program == "spl-token" {
                if let Ok(TokenAccountType::Account(ui_token_account)) =
                    serde_json::from_value(parsed_account.parsed)
                {
                    let _frozen = ui_token_account.state == UiAccountState::Frozen;

                    let token = ui_token_account
                        .mint
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid mint: {}", err));
                    let token_account = keyed_account
                        .pubkey
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid token account: {}", err));
                    let token_amount = ui_token_account
                        .token_amount
                        .amount
                        .parse::<u64>()
                        .unwrap_or_else(|err| panic!("Invalid token amount: {}", err));

                    let _close_authority = ui_token_account.close_authority.map_or(*owner, |s| {
                        s.parse::<Pubkey>()
                            .unwrap_or_else(|err| panic!("Invalid close authority: {}", err))
                    });

                    if ui_token_account.token_amount.decimals == 0 && token_amount == 1 {
                        let (position_pda, _) = Pubkey::find_program_address(
                            &[
                                raydium_amm_v3::states::POSITION_SEED.as_bytes(),
                                token.to_bytes().as_ref(),
                            ],
                            &raydium_amm_v3_program,
                        );
                        nft_account.push(TokenInfo {
                            key: token_account,
                            mint: token,
                            amount: token_amount,
                            decimals: ui_token_account.token_amount.decimals,
                        });
                        user_position_account.push(position_pda);
                    }
                }
            }
        }
    }
    (nft_account, user_position_account)
}

fn main() -> Result<()> {
    println!("Starting...");
    let client_config = "client_config.ini";
    let mut pool_config = load_cfg(&client_config.to_string()).unwrap();
    // Admin and cluster params.
    let payer = read_keypair_file(&pool_config.payer_path)?;
    let admin = read_keypair_file(&pool_config.admin_path)?;
    // solana rpc client
    let rpc_client = RpcClient::new(pool_config.http_url.to_string());

    // anchor client.
    let anchor_config = pool_config.clone();
    let url = Cluster::Custom(anchor_config.http_url, anchor_config.ws_url);
    let wallet = read_keypair_file(&pool_config.payer_path)?;
    let anchor_client = Client::new(url, Rc::new(wallet));
    loop {
        println!("input command:");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        let v: Vec<&str> = line.trim().split(' ').collect();
        match &v[0][..] {
            "mint0" => {
                let keypair_path = "KeyPairs/mint0_keypair.json";
                if !path_is_exist(keypair_path) {
                    if v.len() == 2 {
                        let decimals = v[1].parse::<u64>().unwrap();
                        let mint0 = Keypair::generate(&mut OsRng);
                        let create_and_init_instr = create_and_init_mint_instr(
                            &pool_config.clone(),
                            &mint0.pubkey(),
                            &payer.pubkey(),
                            decimals as u8,
                        )?;
                        // send
                        let signers = vec![&payer, &mint0];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &create_and_init_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);

                        write_keypair_file(&mint0, keypair_path).unwrap();
                        println!("mint0: {}", &mint0.pubkey());
                        pool_config.mint0 = Some(mint0.pubkey());
                    } else {
                        println!("invalid command: [mint0 decimals]");
                    }
                } else {
                    let mint0 = read_keypair_file(keypair_path).unwrap();
                    println!("mint0: {}", &mint0.pubkey());
                    pool_config.mint0 = Some(mint0.pubkey());
                }
            }
            "mint1" => {
                let keypair_path = "KeyPairs/mint1_keypair.json";
                if !path_is_exist(keypair_path) {
                    if v.len() == 2 {
                        let decimals = v[1].parse::<u64>().unwrap();
                        let mint1 = Keypair::generate(&mut OsRng);
                        let create_and_init_instr = create_and_init_mint_instr(
                            &pool_config.clone(),
                            &mint1.pubkey(),
                            &payer.pubkey(),
                            decimals as u8,
                        )?;

                        // send
                        let signers = vec![&payer, &mint1];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &create_and_init_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);

                        write_keypair_file(&mint1, keypair_path).unwrap();
                        println!("mint1: {}", &mint1.pubkey());
                        pool_config.mint1 = Some(mint1.pubkey());
                    } else {
                        println!("invalid command: [mint1 decimals]");
                    }
                } else {
                    let mint1 = read_keypair_file(keypair_path).unwrap();
                    println!("mint1: {}", &mint1.pubkey());
                    pool_config.mint1 = Some(mint1.pubkey());
                }
            }
            "create_ata_token" => {
                if v.len() == 3 {
                    let mint = Pubkey::from_str(&v[1]).unwrap();
                    let owner = Pubkey::from_str(&v[2]).unwrap();
                    let create_ata_instr =
                        create_ata_token_account_instr(&pool_config.clone(), &mint, &owner)?;
                    // send
                    let signers = vec![&payer];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &create_ata_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                    println!("invalid command: [create_ata_token mint owner]");
                }
            }
            "ptoken" => {
                if v.len() == 2 {
                    let token = Pubkey::from_str(&v[1]).unwrap();
                    let cfg = pool_config.clone();
                    let client = RpcClient::new(cfg.http_url.to_string());
                    let token_data = &mut client.get_account_data(&token)?;
                    println!("token_data:{:?}", token_data);
                } else {
                    println!("invalid command: [ptoken token]");
                }
            }
            "mint_to" => {
                if v.len() == 4 {
                    let mint = Pubkey::from_str(&v[1]).unwrap();
                    let to_token = Pubkey::from_str(&v[2]).unwrap();
                    let amount = v[3].parse::<u64>().unwrap();
                    let mint_to_instr = spl_token_mint_to_instr(
                        &pool_config.clone(),
                        &mint,
                        &to_token,
                        amount,
                        &payer,
                    )?;
                    // send
                    let signers = vec![&payer];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &mint_to_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                    println!("invalid command: [mint_to mint to_token amount]");
                }
            }
            "create_config" | "ccfg" => {
                if v.len() == 5 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let tick_spacing = v[2].parse::<u16>().unwrap();
                    let protocol_fee_rate = v[3].parse::<u32>().unwrap();
                    let trade_fee_rate = v[4].parse::<u32>().unwrap();
                    let create_instr = create_amm_config_instr(
                        &pool_config.clone(),
                        config_index,
                        tick_spacing,
                        protocol_fee_rate,
                        trade_fee_rate,
                    )?;
                    // send
                    let signers = vec![&payer, &admin];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &create_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                    println!("invalid command: [ccfg mint protocol_fee_rate]");
                }
            }
            "pcfg" => {
                if v.len() == 2 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let (amm_config_key, __bump) = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                            &config_index.to_be_bytes(),
                        ],
                        &program.id(),
                    );
                    println!("{}", amm_config_key);
                    let amm_config_account: raydium_amm_v3::states::AmmConfig =
                        program.account(amm_config_key)?;
                    println!("{:#?}", amm_config_account);
                } else {
                    println!("invalid command: [pcfg config_index]");
                }
            }
            "update_amm_cfg" => {
                if v.len() == 3 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let flag = v[2].parse::<u8>().unwrap();
                    let mut new_owner = Pubkey::default();
                    let mut trade_fee_rate = 0;
                    let mut protocol_fee_rate = 0;
                    if flag == 0 {
                        new_owner = Pubkey::from_str(&v[3]).unwrap();
                    } else if flag == 1 {
                        trade_fee_rate = v[3].parse::<u32>().unwrap();
                    } else {
                        protocol_fee_rate = v[3].parse::<u32>().unwrap();
                    }
                    let (amm_config_key, __bump) = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                            &config_index.to_be_bytes(),
                        ],
                        &pool_config.raydium_v3_program,
                    );
                    let update_amm_config_instr = update_amm_config_instr(
                        &pool_config.clone(),
                        amm_config_key,
                        new_owner,
                        trade_fee_rate,
                        protocol_fee_rate,
                        flag,
                    )?;
                    // send
                    let signers = vec![&payer, &admin];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &update_amm_config_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                    println!("invalid command: [set_new_cfg_owner config_index new_owner]");
                }
            }
            "cmp_key" => {
                if v.len() == 3 {
                    let token_mint_0 = Pubkey::from_str(&v[1]).unwrap();
                    let token_mint_1 = Pubkey::from_str(&v[2]).unwrap();
                    if token_mint_0 < token_mint_1 {
                        println!("mint_0: {}", token_mint_0);
                        println!("mint_1: {}", token_mint_1);
                    } else {
                        println!("mint_0: {}", token_mint_1);
                        println!("mint_1: {}", token_mint_0);
                    }
                } else {
                    println!("cmp_key mint mint");
                }
            }
            "price_to_tick" => {
                if v.len() == 2 {
                    let price = v[1].parse::<f64>().unwrap();
                    let tick = price_to_tick(price);
                    println!("price:{}, tick:{}", price, tick);
                } else {
                    println!("price_to_tick price");
                }
            }
            "tick_to_price" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let price = tick_to_price(tick);
                    println!("price:{}, tick:{}", price, tick);
                } else {
                    println!("tick_to_price tick");
                }
            }
            "tick_with_spacing" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let tick_spacing = v[2].parse::<i32>().unwrap();
                    let tick_with_spacing = tick_with_spacing(tick, tick_spacing);
                    println!("tick:{}, tick_with_spacing:{}", tick, tick_with_spacing);
                } else {
                    println!("tick_with_spacing tick tick_spacing");
                }
            }
            "tick_array_start_index" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let tick_spacing = v[2].parse::<i32>().unwrap();
                    let tick_array_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick,
                            tick_spacing,
                        );
                    println!(
                        "tick:{}, tick_array_start_index:{}",
                        tick, tick_array_start_index
                    );
                } else {
                    println!("tick_array_start_index tick tick_spacing");
                }
            }
            "liquidity_to_amounts" => {
                let program = anchor_client.program(pool_config.raydium_v3_program);
                println!("{}", pool_config.pool_id_account.unwrap());
                let pool_account: raydium_amm_v3::states::PoolState =
                    program.account(pool_config.pool_id_account.unwrap())?;
                if v.len() == 4 {
                    let tick_upper = v[1].parse::<i32>().unwrap();
                    let tick_lower = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<i128>().unwrap();
                    let amounts = raydium_amm_v3::libraries::get_amounts_delta_signed(
                        pool_account.tick_current,
                        tick_lower,
                        tick_upper,
                        liquidity,
                    )?;
                    println!("amount_0:{}, amount_1:{}", amounts.0, amounts.1);
                }
            }
            "create_pool" | "cpool" => {
                if v.len() == 3 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let price = v[2].parse::<f64>().unwrap();
                    let load_pubkeys = vec![pool_config.mint0.unwrap(), pool_config.mint1.unwrap()];
                    let rsps = rpc_client.get_multiple_accounts(&load_pubkeys)?;
                    let mint0_account =
                        spl_token::state::Mint::unpack(&rsps[0].as_ref().unwrap().data).unwrap();
                    let mint1_account =
                        spl_token::state::Mint::unpack(&rsps[1].as_ref().unwrap().data).unwrap();
                    let sqrt_price_x64 = price_to_sqrt_price_x64(
                        price,
                        mint0_account.decimals,
                        mint1_account.decimals,
                    );
                    let (amm_config_key, __bump) = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                            &config_index.to_be_bytes(),
                        ],
                        &pool_config.raydium_v3_program,
                    );
                    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64).unwrap();
                    println!(
                        "tick:{}, price:{}, sqrt_price_x64:{}, amm_config_key:{}",
                        tick, price, sqrt_price_x64, amm_config_key
                    );
                    let observation_account = Keypair::generate(&mut OsRng);
                    let mut create_observation_instr = create_account_rent_exmpt_instr(
                        &pool_config.clone(),
                        &observation_account.pubkey(),
                        pool_config.raydium_v3_program,
                        raydium_amm_v3::states::ObservationState::LEN,
                    )?;
                    let create_pool_instr = create_pool_instr(
                        &pool_config.clone(),
                        amm_config_key,
                        observation_account.pubkey(),
                        pool_config.mint0.unwrap(),
                        pool_config.mint1.unwrap(),
                        sqrt_price_x64,
                    )?;
                    create_observation_instr.extend(create_pool_instr);

                    // send
                    let signers = vec![&payer, &observation_account];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &create_observation_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                    println!("invalid command: [create_pool config_index tick_spacing]");
                }
            }
            "admin_reset_sqrt_price" => {
                if v.len() == 2 {
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    println!("{}", pool_config.pool_id_account.unwrap());
                    let pool_account: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;
                    let price = v[2].parse::<f64>().unwrap();
                    let sqrt_price_x64 = price_to_sqrt_price_x64(
                        price,
                        pool_account.mint_0_decimals,
                        pool_account.mint_1_decimals,
                    );
                    let admin_reset_sqrt_price_instr = admin_reset_sqrt_price_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        pool_account.token_vault_0,
                        pool_account.token_vault_1,
                        sqrt_price_x64,
                    )
                    .unwrap();
                    // send
                    let signers = vec![&payer, &admin];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &admin_reset_sqrt_price_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                } else {
                }
            }
            "ppool" => {
                let program = anchor_client.program(pool_config.raydium_v3_program);
                println!("{}", pool_config.pool_id_account.unwrap());
                let pool_account: raydium_amm_v3::states::PoolState =
                    program.account(pool_config.pool_id_account.unwrap())?;
                println!("{:#?}", pool_account);
            }
            "open_position" | "open" => {
                if v.len() == 7 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<u128>().unwrap();
                    let amount_0_max = v[4].parse::<u64>().unwrap();
                    let amount_1_max = v[5].parse::<u64>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_array_lower_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_upper_index,
                            pool.tick_spacing.into(),
                        );
                    // load position
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(
                        &rpc_client,
                        &payer.pubkey(),
                        &pool_config.raydium_v3_program,
                    );
                    let rsps = rpc_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_, data, _) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe {
                                    std::mem::transmute::<
                                        &[u8; std::mem::size_of::<
                                            raydium_amm_v3::states::PersonalPositionState,
                                        >()],
                                        &raydium_amm_v3::states::PersonalPositionState,
                                    >(data)
                                };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position =
                        raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == pool_config.pool_id_account.unwrap()
                            && position.tick_lower_index == tick_lower_index
                            && position.tick_upper_index == tick_upper_index
                        {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint == Pubkey::default() {
                        // personal position not exist
                        // new nft mint
                        let nft_mint = Keypair::generate(&mut OsRng);
                        let open_position_instr = open_position_instr(
                            &pool_config.clone(),
                            pool_config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            nft_mint.pubkey(),
                            payer.pubkey(),
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint0.unwrap(),
                            ),
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint1.unwrap(),
                            ),
                            liquidity,
                            amount_0_max,
                            amount_1_max,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?;
                        // send
                        let signers = vec![&payer];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &open_position_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);
                    } else {
                        // personal position exist
                        println!("personal position exist:{:?}", find_position);
                    }
                } else {
                    println!("invalid command: [open_position tick_lower_index tick_upper_index liquidity amount_0_max amount_1_max]");
                }
            }
            "increase_liquidity" => {
                if v.len() == 7 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<u128>().unwrap();
                    let amount_0_max = v[4].parse::<u64>().unwrap();
                    let amount_1_max = v[5].parse::<u64>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_array_lower_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_upper_index,
                            pool.tick_spacing.into(),
                        );
                    // load position
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(
                        &rpc_client,
                        &payer.pubkey(),
                        &pool_config.raydium_v3_program,
                    );
                    let rsps = rpc_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_, data, _) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe {
                                    std::mem::transmute::<
                                        &[u8; std::mem::size_of::<
                                            raydium_amm_v3::states::PersonalPositionState,
                                        >()],
                                        &raydium_amm_v3::states::PersonalPositionState,
                                    >(data)
                                };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position =
                        raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == pool_config.pool_id_account.unwrap()
                            && position.tick_lower_index == tick_lower_index
                            && position.tick_upper_index == tick_upper_index
                        {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint != Pubkey::default()
                        && find_position.pool_id == pool_config.pool_id_account.unwrap()
                    {
                        // personal position exist
                        let increase_instr = increase_liquidity_instr(
                            &pool_config.clone(),
                            pool_config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            find_position.nft_mint,
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint0.unwrap(),
                            ),
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint1.unwrap(),
                            ),
                            liquidity,
                            amount_0_max,
                            amount_1_max,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?;
                        // send
                        let signers = vec![&payer];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &increase_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);
                    } else {
                        // personal position not exist
                        println!("personal position exist:{:?}", find_position);
                    }
                } else {
                    println!("invalid command: [increase_liquidity tick_lower_index tick_upper_index liquidity amount_0_max amount_1_max]");
                }
            }
            "decrease_liquidity" => {
                if v.len() == 6 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<u128>().unwrap();
                    let amount_0_min = v[4].parse::<u64>().unwrap();
                    let amount_1_min = v[5].parse::<u64>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_array_lower_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick_upper_index,
                            pool.tick_spacing.into(),
                        );
                    // load position
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(
                        &rpc_client,
                        &payer.pubkey(),
                        &pool_config.raydium_v3_program,
                    );
                    let rsps = rpc_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_, data, _) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe {
                                    std::mem::transmute::<
                                        &[u8; std::mem::size_of::<
                                            raydium_amm_v3::states::PersonalPositionState,
                                        >()],
                                        &raydium_amm_v3::states::PersonalPositionState,
                                    >(data)
                                };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position =
                        raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == pool_config.pool_id_account.unwrap()
                            && position.tick_lower_index == tick_lower_index
                            && position.tick_upper_index == tick_upper_index
                        {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint != Pubkey::default()
                        && find_position.pool_id == pool_config.pool_id_account.unwrap()
                    {
                        // personal position exist
                        let decrease_instr = decrease_liquidity_instr(
                            &pool_config.clone(),
                            pool_config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            find_position.nft_mint,
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint0.unwrap(),
                            ),
                            spl_associated_token_account::get_associated_token_address(
                                &payer.pubkey(),
                                &pool_config.mint1.unwrap(),
                            ),
                            liquidity,
                            amount_0_min,
                            amount_1_min,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?;
                        // send
                        let signers = vec![&payer];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &decrease_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);
                    } else {
                        // personal position not exist
                        println!("personal position exist:{:?}", find_position);
                    }
                } else {
                    println!("invalid command: [decrease_liquidity tick_lower_index tick_upper_index liquidity amount_0_min amount_1_min]");
                }
            }
            "ptick_state" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    // load pool to get tick_spacing
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_array_start_index =
                        raydium_amm_v3::states::TickArrayState::get_arrary_start_index(
                            tick,
                            pool.tick_spacing.into(),
                        );
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let (tick_array_key, __bump) = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                            pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                            &tick_array_start_index.to_be_bytes(),
                        ],
                        &program.id(),
                    );
                    let mut tick_array_account: raydium_amm_v3::states::TickArrayState =
                        program.account(tick_array_key)?;
                    let tick_state = tick_array_account
                        .get_tick_state_mut(tick, pool.tick_spacing.into())
                        .unwrap();
                    println!("{:?}", tick_state);
                }
            }
            "single_swap" => {
                let tick1 = tick_math::get_tick_at_sqrt_price(18446744073709551616)?;
                let tick2 = tick_math::get_tick_at_sqrt_price(18446744073709541616)?;
                println!("tick1:{}, tick2:{}", tick1, tick2);
                let ret = raydium_amm_v3::libraries::get_amount_0_for_liquidity(
                    18446744073709551616,
                    18446744073709541616,
                    52022602764,
                );
                println!("{}", ret);
                // load pool to get observation
                let program = anchor_client.program(pool_config.raydium_v3_program);
                let _pool: raydium_amm_v3::states::PoolState =
                    program.account(pool_config.pool_id_account.unwrap())?;
            }
            "tick_to_x64" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let sqrt_price_x64 = tick_math::get_sqrt_price_at_tick(tick)?;
                    let sqrt_price_f = (sqrt_price_x64 >> fixed_point_64::RESOLUTION) as f64
                        + (sqrt_price_x64 % fixed_point_64::Q64) as f64
                            / fixed_point_64::Q64 as f64;
                    println!("{}-{}", sqrt_price_x64, sqrt_price_f * sqrt_price_f);
                }
            }
            "sqrt_price_x64_to_tick" => {
                if v.len() == 2 {
                    let sqrt_price_x64 = v[1].parse::<u128>().unwrap();
                    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64)?;
                    println!("sqrt_price_x64:{}, tick:{}", sqrt_price_x64, tick);
                }
            }
            "sqrt_price_x64_to_tick_by_self" => {
                if v.len() == 2 {
                    let sqrt_price_x64 = v[1].parse::<u128>().unwrap();
                    let sqrt_price_f = (sqrt_price_x64 >> fixed_point_64::RESOLUTION) as f64
                        + (sqrt_price_x64 % fixed_point_64::Q64) as f64
                            / fixed_point_64::Q64 as f64;
                    let tick = (sqrt_price_f * sqrt_price_f).log(Q_RATIO) as i32;
                    println!(
                        "tick:{}, sqrt_price_f:{}, price:{}",
                        tick,
                        sqrt_price_f,
                        sqrt_price_f * sqrt_price_f
                    );
                }
            }
            "tick_test" => {
                if v.len() == 2 {
                    let min = v[1].parse::<i32>().unwrap();
                    let price = (2.0 as f64).powi(min);
                    let tick = price.log(Q_RATIO) as i32;
                    println!("tick:{}, price:{}", tick, price);

                    let price = (2.0 as f64).powi(min / 2);
                    let price_x64 = price * fixed_point_64::Q64 as f64;
                    println!("price_x64:{}", price_x64);
                }
            }
            _ => {
                println!("command not exist");
            }
        }
    }
}
