#![allow(dead_code)]
use anchor_client::{Client, Cluster};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer}
};
use solana_client::{
    rpc_client::RpcClient,
    rpc_request::TokenAccountsFilter,
};
use solana_account_decoder::{
    parse_token::{TokenAccountType, UiAccountState},
    UiAccountData,
};
use anyhow::{format_err, Result};

use std::rc::Rc;
use std::str::FromStr;
use std::path::{Path};
use rand::rngs::OsRng;
use arrayref::{ array_refs };
use configparser::ini::Ini;

mod instructions;
use instructions::token_instructions::*;
use instructions::amm_instructions::*;
use raydium_amm_v3::{
    libraries::{tick_math, fixed_point_64},
};

const Q_RATIO: f64 = 1.0001;
fn tick_to_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick)
}
fn price_to_tick(price: f64) -> i32 {
    price.log(Q_RATIO) as i32
}
fn tick_to_sqrt_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick).sqrt()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClientConfig {
    http_url: String,
    ws_url: String,
    payer_path: String,
    admin_path: String,
    raydium_v3_program: Pubkey,
    amm_config_key: Pubkey,

    mint0: Option<Pubkey>,
    mint1: Option<Pubkey>,
    pool_id_account: Option<Pubkey>,
    fee: u32,
    tick_spacing: u16,
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
    let (amm_config_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes()], &raydium_v3_program);

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
    let fee = config.getuint("Pool", "fee").unwrap().unwrap() as u32;
    let tick_spacing = config.getuint("Pool", "tick_spacing").unwrap().unwrap() as u16;

    let pool_id_account = if mint0 != None && mint1 != None {
        Some(Pubkey::find_program_address(&[raydium_amm_v3::states::POOL_SEED.as_bytes(), amm_config_key.to_bytes().as_ref(), mint0.unwrap().to_bytes().as_ref(), mint1.unwrap().to_bytes().as_ref(), &fee.to_be_bytes()], &raydium_v3_program).0)
    }
    else {
        None
    };

    Ok(ClientConfig{
        http_url,
        ws_url,
        payer_path,
        admin_path,
        raydium_v3_program,
        amm_config_key,
        mint0,
        mint1,
        pool_id_account,
        fee,
        tick_spacing,
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

#[derive(Clone, Debug, PartialEq, Eq,)]
struct TokenInfo {
    key: Pubkey,
    mint: Pubkey,
    amount: u64,
    decimals: u8,
}
fn get_nft_account_and_position_by_owner(
    client: &RpcClient,
    owner: &Pubkey,
    raydium_amm_v3_program: &Pubkey
)-> (Vec<TokenInfo>, Vec<Pubkey>) {
    let all_tokens = client.get_token_accounts_by_owner(owner, TokenAccountsFilter::ProgramId(spl_token::id())).unwrap();
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
                        let (position_pda, _) = Pubkey::find_program_address(&[raydium_amm_v3::states::POSITION_SEED.as_bytes(), token.to_bytes().as_ref()], &raydium_amm_v3_program);
                        nft_account.push(TokenInfo{
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
    let config = pool_config.clone();
    // Wallet, Admin and cluster params.
    let payer = read_keypair_file(&config.payer_path)?;
    let admin = read_keypair_file(&config.admin_path)?;
    let url = Cluster::Custom(config.http_url, config.ws_url);
    // Client.
    let client = Client::new_with_options(url, Rc::new(payer), CommitmentConfig::processed());
    let payer = read_keypair_file(&config.payer_path)?;
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
                        let (mint0, _) = create_and_init_mint(&client, &payer.pubkey(), decimals as u8)?;
                        write_keypair_file(&mint0, keypair_path).unwrap();
                        println!("mint0: {}", &mint0.pubkey());
                        pool_config.mint0 = Some(mint0.pubkey());
                    }
                    else {
                        println!("invalid command: [mint0 decimals]");
                    }
                }
                else {
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
                        let (mint1, _) = create_and_init_mint(&client, &payer.pubkey(), decimals as u8)?;
                        write_keypair_file(&mint1, keypair_path).unwrap();
                        println!("mint1: {}", &mint1.pubkey());
                        pool_config.mint1 = Some(mint1.pubkey());
                    }
                    else {
                        println!("invalid command: [mint1 decimals]");
                    }
                }
                else {
                    let mint1 = read_keypair_file(keypair_path).unwrap();
                    println!("mint1: {}", &mint1.pubkey());
                    pool_config.mint1 = Some(mint1.pubkey());
                }
            }
            "create_ata_token" => {
                if v.len() == 3 {
                    let mint = Pubkey::from_str(&v[1]).unwrap();
                    let owner = Pubkey::from_str(&v[2]).unwrap();
                    let sig = create_ata_token_account(&client, &mint, &owner)?;
                    println!("sig:{}", sig);
                }
                else {
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
                }
                else {
                    println!("invalid command: [ptoken token]");
                }
            }
            "mint_to" => {
                if v.len() == 4 {
                    let mint = Pubkey::from_str(&v[1]).unwrap();
                    let to_token = Pubkey::from_str(&v[2]).unwrap();
                    let amount = v[3].parse::<u64>().unwrap();
                    let sig = spl_token_mint_to(&client, &mint, &to_token, amount, &payer)?;
                    println!("sig:{}", sig);
                }
                else {
                    println!("invalid command: [mint_to mint to_token amount]");
                }
            }
            "create_config" | "ccfg" => {
                if v.len() == 5 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let tick_spacing = v[1].parse::<u16>().unwrap();
                    let protocol_fee_rate = v[1].parse::<u32>().unwrap();
                    let trade_fee_rate = v[1].parse::<u32>().unwrap();
                    let sig = create_amm_config_tx(&client, &config.raydium_v3_program, &admin, config_index, tick_spacing, protocol_fee_rate, trade_fee_rate)?;
                    println!("sig:{}", sig);
                }
                else {
                    println!("invalid command: [ccfg mint protocol_fee_rate]");
                }
            }
            "pcfg" => {
                if v.len() == 2 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let program = client.program(config.raydium_v3_program);
                    let (amm_config_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(), &config_index.to_be_bytes()], &program.id());
                    println!("{}", amm_config_key);
                    let amm_config_account: raydium_amm_v3::states::AmmConfig = program.account(amm_config_key)?;
                    println!("{:#?}", amm_config_account);
                }
                else {
                    println!("invalid command: [pcfg config_index]");
                }
            }
            "set_new_cfg_owner" => {
                if v.len() == 3 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let new_owner = Pubkey::from_str(&v[2]).unwrap();
                    let (amm_config_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(), &config_index.to_be_bytes()], &config.raydium_v3_program);
                    let sig = set_new_config_owner_tx(&client, &config.raydium_v3_program, amm_config_key, &admin, &new_owner)?;
                    println!("sig:{}", sig);
                }
                else {
                    println!("invalid command: [set_new_cfg_owner config_index new_owner]");
                }
            }
            "set_protocol_fee_rate" => {
                if v.len() == 3 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let protocol_fee_rate = v[2].parse::<u32>().unwrap();
                    let (amm_config_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(), &config_index.to_be_bytes()], &config.raydium_v3_program);
                    let sig = set_protocol_fee_rate_tx(&client, &config.raydium_v3_program, amm_config_key, &admin, protocol_fee_rate)?;
                    println!("sig:{}", sig);
                }
                else {
                    println!("invalid command: [set_protocol_fee_rate config_index protocol_fee_rate]");
                }
            }
            "create_pool" | "cpool" => {
                if v.len() == 3 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let tick = v[2].parse::<i32>().unwrap();
                    let price = tick_to_price(tick);
                    let sqrt_price_x64 = tick_math::get_sqrt_ratio_at_tick(tick)?;
                    let (amm_config_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(), &config_index.to_be_bytes()], &config.raydium_v3_program);
                    println!("tick:{}, price:{}, sqrt_price_x64:{}, amm_config_key:{}", tick, price, sqrt_price_x64, amm_config_key);
                    let sig = create_pool_tx(&client, config.raydium_v3_program, amm_config_key, config.mint0.unwrap(), config.mint1.unwrap(), sqrt_price_x64)?;
                    println!("sig:{}", sig);
                }
                else {
                    println!("invalid command: [create_pool config_index tick_spacing]");
                }
            }
            "ppool" => {
                let program = client.program(config.raydium_v3_program);
                println!("{}", config.pool_id_account.unwrap());
                let pool_account: raydium_amm_v3::states::PoolState = program.account(config.pool_id_account.unwrap())?;
                println!("{:#?}", pool_account);
            }
            "open_position" | "open" => {
                if v.len() == 7 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let amount_0_desired = v[3].parse::<u64>().unwrap();
                    let amount_1_desired = v[4].parse::<u64>().unwrap();
                    let amount_0_min = v[5].parse::<u64>().unwrap();
                    let amount_1_min = v[6].parse::<u64>().unwrap();

                    let tick_array_lower_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_lower_index, config.tick_spacing.into());
                    let tick_array_upper_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_upper_index, config.tick_spacing.into());
                    // load pool to get observation
                    let program = client.program(config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState = program.account(config.pool_id_account.unwrap())?;
                    // load position
                    let cfg = pool_config.clone();
                    let load_client = RpcClient::new(cfg.http_url.to_string());
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(&load_client, &payer.pubkey(), &cfg.raydium_v3_program);
                    let rsps = load_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_,data,_) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe { std::mem::transmute::<&[u8; std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>()], &raydium_amm_v3::states::PersonalPositionState>(data) };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position = raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == cfg.pool_id_account.unwrap() && position.tick_lower_index == tick_lower_index && position.tick_upper_index == tick_upper_index {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint == Pubkey::default() {
                        // personal position not exist
                        // new nft mint
                        let nft_mint = Keypair::generate(&mut OsRng);
                        open_position_tx(
                            &client,
                            config.raydium_v3_program,
                            pool.amm_config,
                            config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            nft_mint.pubkey(),
                            payer.pubkey(),
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint0.unwrap()),
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint1.unwrap()),
                            amount_0_desired,
                            amount_1_desired,
                            amount_0_min,
                            amount_1_min,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?; 
                    }
                    else {
                        // personal position exist
                        println!("personal position exist:{:?}", find_position);
                    }
                }
                else {
                    println!("invalid command: [open_position tick_lower_index tick_upper_index amount_0_desired amount_1_desired amount_0_min amount_1_min]");
                }
            }
            "increase_liquidity" => {
                if v.len() == 7 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let amount_0_desired = v[3].parse::<u64>().unwrap();
                    let amount_1_desired = v[4].parse::<u64>().unwrap();
                    let amount_0_min = v[5].parse::<u64>().unwrap();
                    let amount_1_min = v[6].parse::<u64>().unwrap();

                    let tick_array_lower_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_lower_index, config.tick_spacing.into());
                    let tick_array_upper_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_upper_index, config.tick_spacing.into());
                    // load pool to get observation
                    let program = client.program(config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState = program.account(config.pool_id_account.unwrap())?;
                    // load position
                    let cfg = pool_config.clone();
                    let load_client = RpcClient::new(cfg.http_url.to_string());
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(&load_client, &payer.pubkey(), &cfg.raydium_v3_program);
                    let rsps = load_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_,data,_) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe { std::mem::transmute::<&[u8; std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>()], &raydium_amm_v3::states::PersonalPositionState>(data) };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position = raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == cfg.pool_id_account.unwrap() && position.tick_lower_index == tick_lower_index && position.tick_upper_index == tick_upper_index {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint != Pubkey::default() && find_position.pool_id == config.pool_id_account.unwrap() {
                        // personal position exist
                        increase_liquidity_tx(
                            &client,
                            config.raydium_v3_program,
                            pool.amm_config,
                            config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            find_position.nft_mint,
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint0.unwrap()),
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint1.unwrap()),
                            amount_0_desired,
                            amount_1_desired,
                            amount_0_min,
                            amount_1_min,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?; 
                    }
                    else {
                        // personal position not exist
                        println!("personal position exist:{:?}", find_position);
                    }
                }
                else {
                    println!("invalid command: [increase_liquidity tick_lower_index tick_upper_index amount_0_desired amount_1_desired amount_0_min amount_1_min]");
                }
            }
            "decrease_liquidity" => {
                if v.len() == 6 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<u128>().unwrap();
                    let amount_0_min = v[4].parse::<u64>().unwrap();
                    let amount_1_min = v[5].parse::<u64>().unwrap();

                    let tick_array_lower_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_lower_index, config.tick_spacing.into());
                    let tick_array_upper_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick_upper_index, config.tick_spacing.into());
                    // load pool to get observation
                    let program = client.program(config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState = program.account(config.pool_id_account.unwrap())?;
                    // load position
                    let cfg = pool_config.clone();
                    let load_client = RpcClient::new(cfg.http_url.to_string());
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(&load_client, &payer.pubkey(), &cfg.raydium_v3_program);
                    let rsps = load_client.get_multiple_accounts(&positions)?;
                    let mut user_positions = Vec::new();
                    for rsp in rsps {
                        match rsp {
                            None => continue,
                            Some(rsp) => {
                                let (_,data,_) = array_refs![&*rsp.data, 8, std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>();..;];
                                let position = unsafe { std::mem::transmute::<&[u8; std::mem::size_of::<raydium_amm_v3::states::PersonalPositionState>()], &raydium_amm_v3::states::PersonalPositionState>(data) };
                                user_positions.push(position);
                            }
                        }
                    }
                    let mut find_position = raydium_amm_v3::states::PersonalPositionState::default();
                    for position in user_positions {
                        if position.pool_id == cfg.pool_id_account.unwrap() && position.tick_lower_index == tick_lower_index && position.tick_upper_index == tick_upper_index {
                            find_position = position.clone();
                        }
                    }
                    if find_position.nft_mint != Pubkey::default() && find_position.pool_id == config.pool_id_account.unwrap() {
                        // personal position exist
                        decrease_liquidity_tx(
                            &client,
                            config.raydium_v3_program,
                            pool.amm_config,
                            config.pool_id_account.unwrap(),
                            pool.token_vault_0,
                            pool.token_vault_1,
                            find_position.nft_mint,
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint0.unwrap()),
                            spl_associated_token_account::get_associated_token_address(&payer.pubkey(), &config.mint1.unwrap()),
                            liquidity,
                            amount_0_min,
                            amount_1_min,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?; 
                    }
                    else {
                        // personal position not exist
                        println!("personal position exist:{:?}", find_position);
                    }
                }
                else {
                    println!("invalid command: [decrease_liquidity tick_lower_index tick_upper_index liquidity amount_0_min amount_1_min]");
                }
            }
            "ptick_state" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let tick_array_start_index = raydium_amm_v3::states::TickArrayState::get_arrary_start_index(tick, config.tick_spacing.into());
                    let program = client.program(config.raydium_v3_program);
                    let (tick_array_key, __bump) = Pubkey::find_program_address(&[raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(), config.pool_id_account.unwrap().to_bytes().as_ref(), &tick_array_start_index.to_be_bytes()], &program.id());
                    let mut tick_array_account: raydium_amm_v3::states::TickArrayState = program.account(tick_array_key)?;
                    let tick_state = tick_array_account.get_tick_state_mut(tick, config.tick_spacing.into()).unwrap();
                    println!("{:?}", tick_state);
                }
            }
            "single_swap" => {
                let tick1 = tick_math::get_tick_at_sqrt_ratio(18446744073709551616)?;
                let tick2 = tick_math::get_tick_at_sqrt_ratio(18446744073709541616)?;
                println!("tick1:{}, tick2:{}", tick1, tick2);
                let ret = raydium_amm_v3::libraries::get_amount_0_for_liquidity(18446744073709551616, 18446744073709541616, 52022602764);
                println!("{}", ret);
                // load pool to get observation
                let program = client.program(config.raydium_v3_program);
                let _pool: raydium_amm_v3::states::PoolState = program.account(config.pool_id_account.unwrap())?;
            }
            "tick_to_x64" => {
                if v.len() == 2 {
                    let tick = v[1].parse::<i32>().unwrap();
                    let sqrt_price_x64 = tick_math::get_sqrt_ratio_at_tick(tick)?;
                    let sqrt_price_f = (sqrt_price_x64 >> fixed_point_64::RESOLUTION) as f64 +  (sqrt_price_x64 % fixed_point_64::Q64) as f64 / fixed_point_64::Q64 as f64;
                    println!("{}-{}", sqrt_price_x64, sqrt_price_f * sqrt_price_f);
                }
            }
            "sqrt_price_x64_to_tick" => {
                if v.len() == 2 {
                    let sqrt_price_x64 = v[1].parse::<u128>().unwrap();
                    let tick = tick_math::get_tick_at_sqrt_ratio(sqrt_price_x64)?;
                    println!("sqrt_price_x64:{}, tick:{}", sqrt_price_x64, tick);
                }
            }
            "sqrt_price_x64_to_tick_by_self" => {
                if v.len() == 2 {
                    let sqrt_price_x64 = v[1].parse::<u128>().unwrap();
                    let sqrt_price_f = (sqrt_price_x64 >> fixed_point_64::RESOLUTION) as f64 +  (sqrt_price_x64 % fixed_point_64::Q64) as f64 / fixed_point_64::Q64 as f64;
                    let tick = (sqrt_price_f * sqrt_price_f).log(Q_RATIO) as i32;
                    println!("tick:{}, sqrt_price_f:{}, price:{}", tick, sqrt_price_f, sqrt_price_f*sqrt_price_f);
                }
            }
            "tick_test" => {
                if v.len() == 2 {
                    let min = v[1].parse::<i32>().unwrap();
                    let price = (2.0 as f64).powi(min);
                    let tick = price.log(Q_RATIO) as i32;
                    println!("tick:{}, price:{}", tick, price);

                    let price = (2.0 as f64).powi(min / 2);
                    let price_x32 = price * fixed_point_64::Q64 as f64;
                    println!("price_x64:{}", price_x32);
                }
            }
            _ => {
                println!("command not exist");
            }
        }
    }
}
