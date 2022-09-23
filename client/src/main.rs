#![allow(dead_code)]
use anchor_client::{Client, Cluster};
use anchor_lang::prelude::AccountMeta;
use anyhow::{format_err, Result};
use arrayref::array_ref;
use configparser::ini::Ini;
use rand::rngs::OsRng;
use solana_account_decoder::{
    parse_token::{TokenAccountType, UiAccountState},
    UiAccountData, UiAccountEncoding,
};
use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
    rpc_request::TokenAccountsFilter,
};
use solana_sdk::{
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;
use std::{collections::VecDeque, mem::size_of};

mod instructions;
use instructions::amm_instructions::*;
use instructions::rpc::*;
use instructions::token_instructions::*;
use instructions::utils::*;
use raydium_amm_v3::{
    libraries::{fixed_point_64, liquidity_math, tick_array_bit_map, tick_math},
    states::{PoolState, TickArrayState},
};
use spl_associated_token_account::get_associated_token_address;

use crate::instructions::utils;
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

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PoolAccounts {
    pool_id: Option<Pubkey>,
    pool_config: Option<Pubkey>,
    pool_observation: Option<Pubkey>,
    pool_protocol_positions: Vec<Pubkey>,
    pool_personal_positions: Vec<Pubkey>,
    pool_tick_arrays: Vec<Pubkey>,
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

fn load_cur_and_next_five_tick_array(
    rpc_client: &RpcClient,
    pool_config: &ClientConfig,
    pool_state: &PoolState,
    zero_for_one: bool,
) -> VecDeque<TickArrayState> {
    let mut current_vaild_tick_array_start_index = pool_state
        .get_first_initialized_tick_array(zero_for_one)
        .unwrap();
    let mut tick_array_keys = Vec::new();
    tick_array_keys.push(
        Pubkey::find_program_address(
            &[
                raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                &current_vaild_tick_array_start_index.to_be_bytes(),
            ],
            &pool_config.raydium_v3_program,
        )
        .0,
    );
    let mut max_array_size = 5;
    while max_array_size != 0 {
        let next_tick_array_index = tick_array_bit_map::next_initialized_tick_array_start_index(
            raydium_amm_v3::libraries::U1024(pool_state.tick_array_bitmap),
            current_vaild_tick_array_start_index,
            pool_state.tick_spacing.into(),
            zero_for_one,
        );
        if next_tick_array_index.is_none() {
            break;
        }
        current_vaild_tick_array_start_index = next_tick_array_index.unwrap();
        tick_array_keys.push(
            Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                    pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                    &current_vaild_tick_array_start_index.to_be_bytes(),
                ],
                &pool_config.raydium_v3_program,
            )
            .0,
        );
        max_array_size -= 1;
    }
    let tick_array_rsps = rpc_client.get_multiple_accounts(&tick_array_keys).unwrap();
    let mut tick_arrays = VecDeque::new();
    for tick_array in tick_array_rsps {
        let tick_array_state =
            deserialize_anchor_account::<raydium_amm_v3::states::TickArrayState>(
                &tick_array.unwrap(),
            )
            .unwrap();
        tick_arrays.push_back(tick_array_state);
    }
    tick_arrays
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
                    let amounts = raydium_amm_v3::libraries::get_delta_amounts_signed(
                        pool_account.tick_current,
                        pool_account.sqrt_price_x64,
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
            "admin_close_personal_position_by_pool" => {
                println!("pool_id:{}", pool_config.pool_id_account.unwrap());
                let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp {
                                offset: 8 + 1 + size_of::<Pubkey>(),
                                bytes: MemcmpEncodedBytes::Bytes(
                                    pool_config.pool_id_account.unwrap().to_bytes().to_vec(),
                                ),
                                encoding: None,
                            }),
                            RpcFilterType::DataSize(
                                raydium_amm_v3::states::PersonalPositionState::LEN as u64,
                            ),
                        ]),
                        account_config: RpcAccountInfoConfig {
                            encoding: Some(UiAccountEncoding::Base64),
                            ..RpcAccountInfoConfig::default()
                        },
                        with_context: Some(false),
                    },
                )?;

                let mut instructions = Vec::new();
                for position in position_accounts_by_pool {
                    let personal_position = deserialize_anchor_account::<
                        raydium_amm_v3::states::PersonalPositionState,
                    >(&position.1)?;
                    if personal_position.pool_id != pool_config.pool_id_account.unwrap() {
                        println!(
                            "personal_position:{} owned by pool:{}",
                            position.0, personal_position.pool_id
                        );
                        panic!("pool id not match");
                    }
                    let admin_close_personal_position_instr = admin_close_personal_position_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        position.0,
                    )
                    .unwrap();
                    instructions.extend(admin_close_personal_position_instr);
                }

                // send
                let signers = vec![&payer, &admin];
                let recent_hash = rpc_client.get_latest_blockhash()?;
                let txn = Transaction::new_signed_with_payer(
                    &instructions,
                    Some(&payer.pubkey()),
                    &signers,
                    recent_hash,
                );
                let signature = send_txn(&rpc_client, &txn, true)?;
                println!("{}", signature);
            }
            "admin_close_protocol_position_by_pool" => {
                let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp {
                                offset: 8 + 1,
                                bytes: MemcmpEncodedBytes::Bytes(
                                    pool_config.pool_id_account.unwrap().to_bytes().to_vec(),
                                ),
                                encoding: None,
                            }),
                            RpcFilterType::DataSize(
                                raydium_amm_v3::states::ProtocolPositionState::LEN as u64,
                            ),
                        ]),
                        account_config: RpcAccountInfoConfig {
                            encoding: Some(UiAccountEncoding::Base64Zstd),
                            ..RpcAccountInfoConfig::default()
                        },
                        with_context: Some(false),
                    },
                )?;

                let mut instructions = Vec::new();
                for position in position_accounts_by_pool {
                    let protocol_position = deserialize_anchor_account::<
                        raydium_amm_v3::states::ProtocolPositionState,
                    >(&position.1)?;
                    if protocol_position.pool_id != pool_config.pool_id_account.unwrap() {
                        println!(
                            "protocol_position:{} owned by pool:{}",
                            position.0, protocol_position.pool_id
                        );
                        panic!("pool id not match");
                    }
                    let admin_close_protocol_position_instr = admin_close_protocol_position_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        position.0,
                    )
                    .unwrap();
                    instructions.extend(admin_close_protocol_position_instr);
                }
                // send
                let signers = vec![&payer, &admin];
                let recent_hash = rpc_client.get_latest_blockhash()?;
                let txn = Transaction::new_signed_with_payer(
                    &instructions,
                    Some(&payer.pubkey()),
                    &signers,
                    recent_hash,
                );
                let signature = send_txn(&rpc_client, &txn, true)?;
                println!("{}", signature);
            }
            "admin_close_tick_array_by_pool" => {
                let tick_arrays_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp {
                                offset: 8,
                                bytes: MemcmpEncodedBytes::Bytes(
                                    pool_config.pool_id_account.unwrap().to_bytes().to_vec(),
                                ),
                                encoding: None,
                            }),
                            RpcFilterType::DataSize(
                                raydium_amm_v3::states::TickArrayState::LEN as u64,
                            ),
                        ]),
                        account_config: RpcAccountInfoConfig {
                            encoding: Some(UiAccountEncoding::Base64Zstd),
                            ..RpcAccountInfoConfig::default()
                        },
                        with_context: Some(false),
                    },
                )?;

                let mut instructions = Vec::new();
                for tick_array in tick_arrays_by_pool {
                    let tick_array_state = deserialize_anchor_account::<
                        raydium_amm_v3::states::TickArrayState,
                    >(&tick_array.1)?;
                    if tick_array_state.pool_id != pool_config.pool_id_account.unwrap() {
                        println!(
                            "tick_array:{} owned by pool:{}",
                            tick_array.0, tick_array_state.pool_id
                        );
                        panic!("pool id not match");
                    }
                    let admin_close_tick_array_instr = admin_close_tick_array_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        tick_array.0,
                    )
                    .unwrap();
                    instructions.extend(admin_close_tick_array_instr);
                }
                // send
                let signers = vec![&payer, &admin];
                let recent_hash = rpc_client.get_latest_blockhash()?;
                let txn = Transaction::new_signed_with_payer(
                    &instructions,
                    Some(&payer.pubkey()),
                    &signers,
                    recent_hash,
                );
                let signature = send_txn(&rpc_client, &txn, true)?;
                println!("{}", signature);
            }
            "admin_close_pool" => {
                if v.len() == 2 {
                    let pool_id = Pubkey::from_str(&v[1]).unwrap();
                    // check all accounts have been closed except amm_config and pool, observation
                    let rsps = rpc_client.get_program_accounts(&pool_config.raydium_v3_program)?;
                    let mut close_pool = PoolAccounts::default();
                    for item in rsps {
                        let data_len = item.1.data.len();
                        if data_len == size_of::<raydium_amm_v3::states::PoolState>() + 8 {
                            println!("pool_id:{}", item.0);
                            if pool_id == item.0 {
                                close_pool.pool_id = Some(pool_id);
                            }
                        } else if data_len == raydium_amm_v3::states::AmmConfig::LEN {
                            println!("config_id:{}", item.0);
                        } else if data_len
                            == size_of::<raydium_amm_v3::states::TickArrayState>() + 8
                        {
                            println!("tick_array:{}", item.0);
                            let tick_array = deserialize_anchor_account::<
                                raydium_amm_v3::states::TickArrayState,
                            >(&item.1)?;
                            if pool_id == tick_array.pool_id {
                                close_pool.pool_tick_arrays.push(item.0);
                            }
                        } else if data_len == raydium_amm_v3::states::ObservationState::LEN {
                            println!("observation:{}", item.0);
                            let pool_observation = deserialize_anchor_account::<
                                raydium_amm_v3::states::ObservationState,
                            >(&item.1)?;
                            if pool_id == pool_observation.pool_id {
                                close_pool.pool_observation = Some(item.0);
                            }
                        } else if data_len == raydium_amm_v3::states::ProtocolPositionState::LEN {
                            println!("protocol_position:{}", item.0);
                            let protocol_position = deserialize_anchor_account::<
                                raydium_amm_v3::states::ProtocolPositionState,
                            >(&item.1)?;
                            if pool_id == protocol_position.pool_id {
                                close_pool.pool_protocol_positions.push(item.0);
                            }
                        } else if data_len == raydium_amm_v3::states::PersonalPositionState::LEN {
                            println!("personal_position:{}", item.0);
                            let personal_position = deserialize_anchor_account::<
                                raydium_amm_v3::states::PersonalPositionState,
                            >(&item.1)?;
                            if pool_id == personal_position.pool_id {
                                close_pool.pool_personal_positions.push(item.0);
                            }
                        }
                    }
                    if close_pool.pool_id.is_some()
                        && close_pool.pool_observation.is_some()
                        && close_pool.pool_protocol_positions.is_empty()
                        && close_pool.pool_personal_positions.is_empty()
                        && close_pool.pool_tick_arrays.is_empty()
                    {
                        let program = anchor_client.program(pool_config.raydium_v3_program);
                        println!("{}", close_pool.pool_id.unwrap());
                        let pool_account: raydium_amm_v3::states::PoolState =
                            program.account(close_pool.pool_id.unwrap())?;

                        let admin_close_pool_instr = admin_close_pool_instr(
                            &pool_config.clone(),
                            close_pool.pool_id.unwrap(),
                            close_pool.pool_observation.unwrap(),
                            pool_account.token_vault_0,
                            pool_account.token_vault_1,
                        )
                        .unwrap();
                        // send
                        let signers = vec![&payer, &admin];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &admin_close_pool_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        let signature = send_txn(&rpc_client, &txn, true)?;
                        println!("{}", signature);
                    } else {
                        println!("close_pool:{:#?}", close_pool);
                    }
                } else {
                    println!("invalid command: [admin_close_pool pool_id]");
                }
            }
            "admin_reset_sqrt_price" => {
                if v.len() == 4 {
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    println!("{}", pool_config.pool_id_account.unwrap());
                    let pool_account: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;
                    let price = v[1].parse::<f64>().unwrap();
                    let receive_token_0 = Pubkey::from_str(&v[2]).unwrap();
                    let receive_token_1 = Pubkey::from_str(&v[3]).unwrap();
                    let sqrt_price_x64 = price_to_sqrt_price_x64(
                        price,
                        pool_account.mint_decimals_0,
                        pool_account.mint_decimals_1,
                    );
                    let tick = tick_math::get_tick_at_sqrt_price(sqrt_price_x64).unwrap();
                    println!(
                        "tick:{}, price:{}, sqrt_price_x64:{}",
                        tick, price, sqrt_price_x64
                    );
                    let admin_reset_sqrt_price_instr = admin_reset_sqrt_price_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        pool_account.token_vault_0,
                        pool_account.token_vault_1,
                        pool_account.observation_key,
                        receive_token_0,
                        receive_token_1,
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
                    println!("invalid command: [admin_reset_sqrt_price price receive_token_0 receive_token_1]");
                }
            }
            "init_reward" => {
                if v.len() == 6 {
                    let reward_index = v[1].parse::<u8>().unwrap();
                    let open_time = v[2].parse::<u64>().unwrap();
                    let end_time = v[3].parse::<u64>().unwrap();
                    let emissions_per_second = v[4].parse::<f64>().unwrap();
                    let reward_token_mint = Pubkey::from_str(&v[5]).unwrap();

                    let emissions_per_second_x64 =
                        (emissions_per_second * fixed_point_64::Q64 as f64) as u128;

                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    println!("{}", pool_config.pool_id_account.unwrap());
                    let pool_account: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let reward_token_vault = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::POOL_REWARD_VAULT_SEED.as_bytes(),
                            pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                            reward_token_mint.to_bytes().as_ref(),
                        ],
                        &program.id(),
                    )
                    .0;
                    let user_reward_token =
                        get_associated_token_address(&admin.pubkey(), &reward_token_mint);
                    let create_instr = initialize_reward_instr(
                        &pool_config.clone(),
                        pool_config.pool_id_account.unwrap(),
                        pool_account.amm_config,
                        reward_token_mint,
                        reward_token_vault,
                        user_reward_token,
                        reward_index,
                        open_time,
                        end_time,
                        emissions_per_second_x64,
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
                    println!("invalid command: [init_reward reward_index open_time, end_time, emissions_per_second_x64, reward_token_mint]");
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
                if v.len() == 5 {
                    let tick_lower_price = v[1].parse::<f64>().unwrap();
                    let tick_upper_price = v[2].parse::<f64>().unwrap();
                    let is_base_0 = v[3].parse::<bool>().unwrap();
                    let imput_amount = v[4].parse::<u64>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_lower_price_x64 = price_to_sqrt_price_x64(
                        tick_lower_price,
                        pool.mint_decimals_0,
                        pool.mint_decimals_1,
                    );
                    let tick_upper_price_x64 = price_to_sqrt_price_x64(
                        tick_upper_price,
                        pool.mint_decimals_0,
                        pool.mint_decimals_1,
                    );
                    let tick_lower_index = tick_with_spacing(
                        tick_math::get_tick_at_sqrt_price(tick_lower_price_x64)?,
                        pool.tick_spacing.into(),
                    );
                    let tick_upper_index = tick_with_spacing(
                        tick_math::get_tick_at_sqrt_price(tick_upper_price_x64)?,
                        pool.tick_spacing.into(),
                    );
                    println!(
                        "tick_lower_index:{}, tick_upper_index:{}",
                        tick_lower_index, tick_upper_index
                    );
                    let tick_lower_price_x64 = tick_math::get_sqrt_price_at_tick(tick_lower_index)?;
                    let tick_upper_price_x64 = tick_math::get_sqrt_price_at_tick(tick_upper_index)?;
                    let liquidity = if is_base_0 {
                        liquidity_math::get_liquidity_from_single_amount_0(
                            pool.sqrt_price_x64,
                            tick_lower_price_x64,
                            tick_upper_price_x64,
                            imput_amount,
                        )
                    } else {
                        liquidity_math::get_liquidity_from_single_amount_1(
                            pool.sqrt_price_x64,
                            tick_lower_price_x64,
                            tick_upper_price_x64,
                            imput_amount,
                        )
                    };
                    let (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
                        pool.tick_current,
                        pool.sqrt_price_x64,
                        tick_lower_index,
                        tick_upper_index,
                        liquidity as i128,
                    )?;
                    println!(
                        "amount_0:{}, amount_1:{}, liquidity:{}",
                        amount_0, amount_1, liquidity
                    );
                    let amount_0_max = amount_0 as u64;
                    let amount_1_max = amount_1 as u64;

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
                                let position = deserialize_anchor_account::<
                                    raydium_amm_v3::states::PersonalPositionState,
                                >(&rsp)?;
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
                    println!("invalid command: [open_position tick_lower_price tick_upper_price is_base_0 imput_amount]");
                }
            }
            "increase_liquidity" => {
                if v.len() == 5 {
                    let tick_lower_price = v[1].parse::<f64>().unwrap();
                    let tick_upper_price = v[2].parse::<f64>().unwrap();
                    let is_base_0 = v[3].parse::<bool>().unwrap();
                    let imput_amount = v[4].parse::<u64>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_lower_price_x64 = price_to_sqrt_price_x64(
                        tick_lower_price,
                        pool.mint_decimals_0,
                        pool.mint_decimals_1,
                    );
                    let tick_upper_price_x64 = price_to_sqrt_price_x64(
                        tick_upper_price,
                        pool.mint_decimals_0,
                        pool.mint_decimals_1,
                    );
                    let tick_lower_index = tick_with_spacing(
                        tick_math::get_tick_at_sqrt_price(tick_lower_price_x64)?,
                        pool.tick_spacing.into(),
                    );
                    let tick_upper_index = tick_with_spacing(
                        tick_math::get_tick_at_sqrt_price(tick_upper_price_x64)?,
                        pool.tick_spacing.into(),
                    );
                    println!(
                        "tick_lower_index:{}, tick_upper_index:{}",
                        tick_lower_index, tick_upper_index
                    );
                    let tick_lower_price_x64 = tick_math::get_sqrt_price_at_tick(tick_lower_index)?;
                    let tick_upper_price_x64 = tick_math::get_sqrt_price_at_tick(tick_upper_index)?;
                    let liquidity = if is_base_0 {
                        liquidity_math::get_liquidity_from_single_amount_0(
                            pool.sqrt_price_x64,
                            tick_lower_price_x64,
                            tick_upper_price_x64,
                            imput_amount,
                        )
                    } else {
                        liquidity_math::get_liquidity_from_single_amount_1(
                            pool.sqrt_price_x64,
                            tick_lower_price_x64,
                            tick_upper_price_x64,
                            imput_amount,
                        )
                    };
                    let (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
                        pool.tick_current,
                        pool.sqrt_price_x64,
                        tick_lower_index,
                        tick_upper_index,
                        liquidity as i128,
                    )?;
                    println!(
                        "amount_0:{}, amount_1:{}, liquidity:{}",
                        amount_0, amount_1, liquidity
                    );
                    let amount_0_max = amount_0 as u64;
                    let amount_1_max = amount_1 as u64;

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
                                let position = deserialize_anchor_account::<
                                    raydium_amm_v3::states::PersonalPositionState,
                                >(&rsp)?;
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
                    println!("invalid command: [increase_liquidity tick_lower_price tick_upper_price is_base_0 imput_amount]");
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
                                let position = deserialize_anchor_account::<
                                    raydium_amm_v3::states::PersonalPositionState,
                                >(&rsp)?;
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
            "swap_base_in" => {
                if v.len() == 4 || v.len() == 5 {
                    let user_input_token = Pubkey::from_str(&v[1]).unwrap();
                    let user_output_token = Pubkey::from_str(&v[2]).unwrap();
                    let amount_in = v[3].parse::<u64>().unwrap();
                    let mut sqrt_price_limit_x64 = None;
                    if v.len() == 5 {
                        sqrt_price_limit_x64 = Some(v[4].parse::<u128>().unwrap());
                    }
                    let is_base_input = true;

                    // load mult account
                    let load_accounts = vec![
                        user_input_token,
                        user_output_token,
                        pool_config.amm_config_key,
                        pool_config.pool_id_account.unwrap(),
                    ];
                    let rsps = rpc_client.get_multiple_accounts(&load_accounts)?;
                    let [user_input_account, user_output_account, amm_config_account, pool_account] =
                        array_ref![rsps, 0, 4];
                    let user_input_state = spl_token::state::Account::unpack(
                        &user_input_account.as_ref().unwrap().data,
                    )
                    .unwrap();
                    let user_output_state = spl_token::state::Account::unpack(
                        &user_output_account.as_ref().unwrap().data,
                    )
                    .unwrap();
                    let amm_config_state =
                        deserialize_anchor_account::<raydium_amm_v3::states::AmmConfig>(
                            amm_config_account.as_ref().unwrap(),
                        )?;
                    let pool_state = deserialize_anchor_account::<raydium_amm_v3::states::PoolState>(
                        pool_account.as_ref().unwrap(),
                    )?;
                    let zero_for_one = user_input_state.mint == pool_state.token_mint_0
                        && user_output_state.mint == pool_state.token_mint_1;
                    // load tick_arrays
                    let mut tick_arrays = load_cur_and_next_five_tick_array(
                        &rpc_client,
                        &pool_config,
                        &pool_state,
                        zero_for_one,
                    );

                    let (other_amount_threshold, mut tick_array_indexs) =
                        utils::get_out_put_amount_and_remaining_accounts(
                            amount_in,
                            sqrt_price_limit_x64,
                            zero_for_one,
                            is_base_input,
                            &amm_config_state,
                            &pool_state,
                            &mut tick_arrays,
                        )
                        .unwrap();

                    let current_or_next_tick_array_key = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                            pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                            &tick_array_indexs.pop_front().unwrap().to_be_bytes(),
                        ],
                        &pool_config.raydium_v3_program,
                    )
                    .0;
                    let remaining_accounts = tick_array_indexs
                        .into_iter()
                        .map(|index| {
                            AccountMeta::new(
                                Pubkey::find_program_address(
                                    &[
                                        raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                                        pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                                        &index.to_be_bytes(),
                                    ],
                                    &pool_config.raydium_v3_program,
                                )
                                .0,
                                false,
                            )
                        })
                        .collect();
                    let swap_instr = swap_instr(
                        &pool_config.clone(),
                        pool_state.amm_config,
                        pool_config.pool_id_account.unwrap(),
                        if zero_for_one {
                            pool_state.token_vault_0
                        } else {
                            pool_state.token_vault_1
                        },
                        if zero_for_one {
                            pool_state.token_vault_1
                        } else {
                            pool_state.token_vault_0
                        },
                        pool_state.observation_key,
                        user_input_token,
                        user_output_token,
                        current_or_next_tick_array_key,
                        remaining_accounts,
                        amount_in,
                        other_amount_threshold,
                        sqrt_price_limit_x64,
                        is_base_input,
                    )
                    .unwrap();
                    // send
                    let signers = vec![&payer];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &swap_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                }
            }
            "swap_base_out" => {
                if v.len() == 4 || v.len() == 5 {
                    let user_input_token = Pubkey::from_str(&v[1]).unwrap();
                    let user_output_token = Pubkey::from_str(&v[2]).unwrap();
                    let amount_in = v[3].parse::<u64>().unwrap();
                    let mut sqrt_price_limit_x64 = None;
                    if v.len() == 5 {
                        sqrt_price_limit_x64 = Some(v[4].parse::<u128>().unwrap());
                    }
                    let is_base_input = false;

                    // load mult account
                    let load_accounts = vec![
                        user_input_token,
                        user_output_token,
                        pool_config.amm_config_key,
                        pool_config.pool_id_account.unwrap(),
                    ];
                    let rsps = rpc_client.get_multiple_accounts(&load_accounts)?;
                    let [user_input_account, user_output_account, amm_config_account, pool_account] =
                        array_ref![rsps, 0, 4];
                    let user_input_state = spl_token::state::Account::unpack(
                        &user_input_account.as_ref().unwrap().data,
                    )
                    .unwrap();
                    let user_output_state = spl_token::state::Account::unpack(
                        &user_output_account.as_ref().unwrap().data,
                    )
                    .unwrap();
                    let amm_config_state =
                        deserialize_anchor_account::<raydium_amm_v3::states::AmmConfig>(
                            amm_config_account.as_ref().unwrap(),
                        )?;
                    let pool_state = deserialize_anchor_account::<raydium_amm_v3::states::PoolState>(
                        pool_account.as_ref().unwrap(),
                    )?;
                    let zero_for_one = user_input_state.mint == pool_state.token_mint_0
                        && user_output_state.mint == pool_state.token_mint_1;
                    // load tick_arrays
                    let mut tick_arrays = load_cur_and_next_five_tick_array(
                        &rpc_client,
                        &pool_config,
                        &pool_state,
                        zero_for_one,
                    );

                    let (other_amount_threshold, mut tick_array_indexs) =
                        utils::get_out_put_amount_and_remaining_accounts(
                            amount_in,
                            sqrt_price_limit_x64,
                            zero_for_one,
                            is_base_input,
                            &amm_config_state,
                            &pool_state,
                            &mut tick_arrays,
                        )
                        .unwrap();

                    let current_or_next_tick_array_key = Pubkey::find_program_address(
                        &[
                            raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                            pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                            &tick_array_indexs.pop_front().unwrap().to_be_bytes(),
                        ],
                        &pool_config.raydium_v3_program,
                    )
                    .0;
                    let remaining_accounts = tick_array_indexs
                        .into_iter()
                        .map(|index| {
                            AccountMeta::new(
                                Pubkey::find_program_address(
                                    &[
                                        raydium_amm_v3::states::TICK_ARRAY_SEED.as_bytes(),
                                        pool_config.pool_id_account.unwrap().to_bytes().as_ref(),
                                        &index.to_be_bytes(),
                                    ],
                                    &pool_config.raydium_v3_program,
                                )
                                .0,
                                false,
                            )
                        })
                        .collect();
                    let swap_instr = swap_instr(
                        &pool_config.clone(),
                        pool_state.amm_config,
                        pool_config.pool_id_account.unwrap(),
                        if zero_for_one {
                            pool_state.token_vault_0
                        } else {
                            pool_state.token_vault_1
                        },
                        if zero_for_one {
                            pool_state.token_vault_1
                        } else {
                            pool_state.token_vault_0
                        },
                        pool_state.observation_key,
                        user_input_token,
                        user_output_token,
                        current_or_next_tick_array_key,
                        remaining_accounts,
                        amount_in,
                        other_amount_threshold,
                        sqrt_price_limit_x64,
                        is_base_input,
                    )
                    .unwrap();
                    // send
                    let signers = vec![&payer];
                    let recent_hash = rpc_client.get_latest_blockhash()?;
                    let txn = Transaction::new_signed_with_payer(
                        &swap_instr,
                        Some(&payer.pubkey()),
                        &signers,
                        recent_hash,
                    );
                    let signature = send_txn(&rpc_client, &txn, true)?;
                    println!("{}", signature);
                }
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
