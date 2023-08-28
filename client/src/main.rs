#![allow(dead_code)]
use anchor_client::{Client, Cluster};
use anchor_lang::prelude::AccountMeta;
use anchor_lang::AnchorDeserialize;
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
    rpc_filter::{Memcmp, RpcFilterType},
    rpc_request::TokenAccountsFilter,
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;
use std::{collections::VecDeque, convert::identity, mem::size_of};

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
    let (_, mut current_vaild_tick_array_start_index) = pool_state
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
                if v.len() == 6 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let tick_spacing = v[2].parse::<u16>().unwrap();
                    let trade_fee_rate = v[3].parse::<u32>().unwrap();
                    let protocol_fee_rate = v[4].parse::<u32>().unwrap();
                    let fund_fee_rate = v[5].parse::<u32>().unwrap();
                    let create_instr = create_amm_config_instr(
                        &pool_config.clone(),
                        config_index,
                        tick_spacing,
                        trade_fee_rate,
                        protocol_fee_rate,
                        fund_fee_rate,
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
                    println!("invalid command: [ccfg index tick_spacing trade_fee_rate protocol_fee_rate fund_fee_rate]");
                }
            }
            "create_operation" => {
                if v.len() == 1 {
                    let create_instr = create_operation_account_instr(&pool_config.clone())?;
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
                    println!("invalid command: [create_operation]");
                }
            }
            "update_operation" => {
                let param = v[1].parse::<u8>().unwrap();
                let mut keys = Vec::new();
                for i in 2..v.len() {
                    keys.push(Pubkey::from_str(&v[i]).unwrap());
                }
                let create_instr =
                    update_operation_account_instr(&pool_config.clone(), param, keys)?;
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
            }
            "poperation" => {
                if v.len() == 1 {
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let (operation_account_key, __bump) = Pubkey::find_program_address(
                        &[raydium_amm_v3::states::OPERATION_SEED.as_bytes()],
                        &program.id(),
                    );
                    println!("{}", operation_account_key);
                    let operation_account: raydium_amm_v3::states::OperationState =
                        program.account(operation_account_key)?;
                    println!("{:#?}", operation_account);
                } else {
                    println!("invalid command: [poperation]");
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
                if v.len() == 4 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let param = v[2].parse::<u8>().unwrap();
                    let mut remaing_accounts = Vec::new();
                    let mut value = 0;
                    let match_param = Some(param);
                    match match_param {
                        Some(0) => value = v[3].parse::<u32>().unwrap(),
                        Some(1) => value = v[3].parse::<u32>().unwrap(),
                        Some(2) => value = v[3].parse::<u32>().unwrap(),
                        Some(3) => {
                            remaing_accounts.push(AccountMeta::new_readonly(
                                Pubkey::from_str(&v[3]).unwrap(),
                                false,
                            ));
                        }
                        Some(4) => {
                            remaing_accounts.push(AccountMeta::new_readonly(
                                Pubkey::from_str(&v[3]).unwrap(),
                                false,
                            ));
                        }
                        _ => panic!("error input"),
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
                        remaing_accounts,
                        param,
                        value,
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
                    let mut token_mint_0 = Pubkey::from_str(&v[1]).unwrap();
                    let mut token_mint_1 = Pubkey::from_str(&v[2]).unwrap();
                    if token_mint_0 > token_mint_1 {
                        std::mem::swap(&mut token_mint_0, &mut token_mint_1);
                    }
                    println!("mint0:{}, mint1:{}", token_mint_0, token_mint_1);
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
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
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
                if v.len() == 6 {
                    let config_index = v[1].parse::<u16>().unwrap();
                    let mut price = v[2].parse::<f64>().unwrap();
                    let mut mint0 = Pubkey::from_str(&v[3]).unwrap();
                    let mut mint1 = Pubkey::from_str(&v[4]).unwrap();
                    let open_time = v[5].parse::<u64>().unwrap();
                    if mint0 > mint1 {
                        std::mem::swap(&mut mint0, &mut mint1);
                        price = 1.0 / price;
                    }
                    println!("mint0:{}, mint1:{}, price:{}", mint0, mint1, price);
                    let load_pubkeys = vec![mint0, mint1];
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
                        mint0,
                        mint1,
                        sqrt_price_x64,
                        open_time,
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
            "p_all_personal_position_by_pool" => {
                println!("pool_id:{}", pool_config.pool_id_account.unwrap());
                let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                                8 + 1 + size_of::<Pubkey>(),
                                &pool_config.pool_id_account.unwrap().to_bytes(),
                            )),
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

                let mut total_fees_owed_0 = 0;
                let mut total_fees_owed_1 = 0;
                let mut total_reward_owed = 0;
                for position in position_accounts_by_pool {
                    let personal_position = deserialize_anchor_account::<
                        raydium_amm_v3::states::PersonalPositionState,
                    >(&position.1)?;
                    if personal_position.pool_id == pool_config.pool_id_account.unwrap() {
                        println!(
                            "personal_position:{}, lower:{}, upper:{}, liquidity:{}, token_fees_owed_0:{}, token_fees_owed_1:{}, reward_amount_owed:{}, fee_growth_inside:{}, fee_growth_inside_1:{}, reward_inside:{}",
                            position.0,
                            personal_position.tick_lower_index,
                            personal_position.tick_upper_index,
                            personal_position.liquidity,
                            personal_position.token_fees_owed_0,
                            personal_position.token_fees_owed_1,
                            personal_position.reward_infos[0].reward_amount_owed,
                            personal_position.fee_growth_inside_0_last_x64,
                            personal_position.fee_growth_inside_1_last_x64,
                            personal_position.reward_infos[0].growth_inside_last_x64,
                        );
                        total_fees_owed_0 += personal_position.token_fees_owed_0;
                        total_fees_owed_1 += personal_position.token_fees_owed_1;
                        total_reward_owed += personal_position.reward_infos[0].reward_amount_owed;
                    }
                }
                println!(
                    "total_fees_owed_0:{}, total_fees_owed_1:{}, total_reward_owed:{}",
                    total_fees_owed_0, total_fees_owed_1, total_reward_owed
                );
            }
            "p_all_protocol_position_by_pool" => {
                let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                                8 + 1,
                                &pool_config.pool_id_account.unwrap().to_bytes(),
                            )),
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

                for position in position_accounts_by_pool {
                    let protocol_position = deserialize_anchor_account::<
                        raydium_amm_v3::states::ProtocolPositionState,
                    >(&position.1)?;
                    if protocol_position.pool_id == pool_config.pool_id_account.unwrap() {
                        println!(
                            "protocol_position:{} lower_index:{}, upper_index:{}",
                            position.0,
                            protocol_position.tick_lower_index,
                            protocol_position.tick_upper_index,
                        );
                    }
                }
            }
            "p_all_tick_array_by_pool" => {
                let tick_arrays_by_pool = rpc_client.get_program_accounts_with_config(
                    &pool_config.raydium_v3_program,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                                8,
                                &pool_config.pool_id_account.unwrap().to_bytes(),
                            )),
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

                for tick_array in tick_arrays_by_pool {
                    let tick_array_state = deserialize_anchor_account::<
                        raydium_amm_v3::states::TickArrayState,
                    >(&tick_array.1)?;
                    if tick_array_state.pool_id == pool_config.pool_id_account.unwrap() {
                        println!(
                            "tick_array:{}, {}, {}",
                            tick_array.0,
                            identity(tick_array_state.start_tick_index),
                            identity(tick_array_state.initialized_tick_count)
                        );
                    }
                }
            }
            "init_reward" => {
                if v.len() == 5 {
                    let open_time = v[1].parse::<u64>().unwrap();
                    let end_time = v[2].parse::<u64>().unwrap();
                    // emissions_per_second is mul 10^^decimals
                    let emissions_per_second = v[3].parse::<f64>().unwrap();
                    let reward_token_mint = Pubkey::from_str(&v[4]).unwrap();

                    let emissions_per_second_x64 =
                        (emissions_per_second * fixed_point_64::Q64 as f64) as u128;

                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    println!("{}", pool_config.pool_id_account.unwrap());
                    let pool_account: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;
                    let operator_account_key = Pubkey::find_program_address(
                        &[raydium_amm_v3::states::OPERATION_SEED.as_bytes()],
                        &program.id(),
                    )
                    .0;

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
                        operator_account_key,
                        reward_token_mint,
                        reward_token_vault,
                        user_reward_token,
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
                    println!("invalid command: [init_reward open_time, end_time, emissions_per_second_x64, reward_token_mint]");
                }
            }
            "set_reward_params" => {
                if v.len() == 6 {
                    let index = v[1].parse::<u8>().unwrap();
                    let open_time = v[2].parse::<u64>().unwrap();
                    let end_time = v[3].parse::<u64>().unwrap();
                    // emissions_per_second is mul 10^^decimals
                    let emissions_per_second = v[4].parse::<f64>().unwrap();
                    let reward_token_mint = Pubkey::from_str(&v[5]).unwrap();
                    let emissions_per_second_x64 =
                        (emissions_per_second * fixed_point_64::Q64 as f64) as u128;

                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    println!("{}", pool_config.pool_id_account.unwrap());
                    let pool_account: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;
                    let operator_account_key = Pubkey::find_program_address(
                        &[raydium_amm_v3::states::OPERATION_SEED.as_bytes()],
                        &program.id(),
                    )
                    .0;

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
                    let create_instr = set_reward_params_instr(
                        &pool_config.clone(),
                        pool_account.amm_config,
                        pool_config.pool_id_account.unwrap(),
                        reward_token_vault,
                        user_reward_token,
                        operator_account_key,
                        index,
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
                    println!("invalid command: [set_reward_params index, open_time, end_time, emissions_per_second_x64, reward_token_mint]");
                }
            }
            "ppool" => {
                let program = anchor_client.program(pool_config.raydium_v3_program);
                let pool_id = if v.len() == 2 {
                    Pubkey::from_str(&v[1]).unwrap()
                } else {
                    pool_config.pool_id_account.unwrap()
                };
                println!("{}", pool_id);
                let pool_account: raydium_amm_v3::states::PoolState = program.account(pool_id)?;
                println!("{:#?}", pool_account);
            }
            "pprotocol" => {
                if v.len() == 2 {
                    let protocol_key = Pubkey::from_str(&v[1]).unwrap();
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let protocol_account: raydium_amm_v3::states::ProtocolPositionState =
                        program.account(protocol_key)?;
                    println!("{:#?}", protocol_account);
                }
            }
            "ppersonal" => {
                if v.len() == 2 {
                    let personal_key = Pubkey::from_str(&v[1]).unwrap();
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let personal_account: raydium_amm_v3::states::PersonalPositionState =
                        program.account(personal_key)?;
                    println!("{:#?}", personal_account);
                }
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
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
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
                        let mut instructions = Vec::new();
                        let request_inits_instr =
                            ComputeBudgetInstruction::set_compute_unit_limit(1400_000u32);
                        instructions.push(request_inits_instr);
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
                        instructions.extend(open_position_instr);
                        // send
                        let signers = vec![&payer, &nft_mint];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &instructions,
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
            "pall_position_by_owner" => {
                if v.len() == 2 {
                    let user_wallet = Pubkey::from_str(&v[1]).unwrap();
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    // load position
                    let (_nft_tokens, positions) = get_nft_account_and_position_by_owner(
                        &rpc_client,
                        &user_wallet,
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
                                let (personal_position_key, __bump) = Pubkey::find_program_address(
                                    &[
                                        raydium_amm_v3::states::POSITION_SEED.as_bytes(),
                                        position.nft_mint.to_bytes().as_ref(),
                                    ],
                                    &program.id(),
                                );
                                println!("id:{}, lower:{}, upper:{}, liquidity:{}, fees_owed_0:{}, fees_owed_1:{}, fee_growth_inside_0:{}, fee_growth_inside_1:{}", personal_position_key, position.tick_lower_index, position.tick_upper_index, position.liquidity, position.token_fees_owed_0, position.token_fees_owed_1, position.fee_growth_inside_0_last_x64, position.fee_growth_inside_1_last_x64);
                                user_positions.push(position);
                            }
                        }
                    }
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
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
                            tick_upper_index,
                            pool.tick_spacing.into(),
                        );
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
                if v.len() == 7 {
                    let tick_lower_index = v[1].parse::<i32>().unwrap();
                    let tick_upper_index = v[2].parse::<i32>().unwrap();
                    let liquidity = v[3].parse::<u128>().unwrap();
                    let amount_0_min = v[4].parse::<u64>().unwrap();
                    let amount_1_min = v[5].parse::<u64>().unwrap();
                    let simulate = v[6].parse::<bool>().unwrap();

                    // load pool to get observation
                    let program = anchor_client.program(pool_config.raydium_v3_program);
                    let pool: raydium_amm_v3::states::PoolState =
                        program.account(pool_config.pool_id_account.unwrap())?;

                    let tick_array_lower_start_index =
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
                            tick_lower_index,
                            pool.tick_spacing.into(),
                        );
                    let tick_array_upper_start_index =
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
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
                            println!("liquidity:{:?}", find_position);
                        }
                    }
                    if find_position.nft_mint != Pubkey::default()
                        && find_position.pool_id == pool_config.pool_id_account.unwrap()
                    {
                        let mut reward_vault_with_user_vault: Vec<(Pubkey, Pubkey)> = Vec::new();
                        for item in pool.reward_infos.into_iter() {
                            if item.token_mint != Pubkey::default() {
                                reward_vault_with_user_vault.push((
                                    item.token_vault,
                                    get_associated_token_address(&payer.pubkey(), &item.token_mint),
                                ));
                            }
                        }
                        let remaining_accounts = reward_vault_with_user_vault
                            .into_iter()
                            .map(|item| AccountMeta::new(item.0, false))
                            .collect();
                        // personal position exist
                        let mut decrease_instr = decrease_liquidity_instr(
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
                            remaining_accounts,
                            liquidity,
                            amount_0_min,
                            amount_1_min,
                            tick_lower_index,
                            tick_upper_index,
                            tick_array_lower_start_index,
                            tick_array_upper_start_index,
                        )?;
                        if liquidity == find_position.liquidity {
                            let close_position_instr = close_personal_position_instr(
                                &pool_config.clone(),
                                find_position.nft_mint,
                            )?;
                            decrease_instr.extend(close_position_instr);
                        }
                        // send
                        let signers = vec![&payer];
                        let recent_hash = rpc_client.get_latest_blockhash()?;
                        let txn = Transaction::new_signed_with_payer(
                            &decrease_instr,
                            Some(&payer.pubkey()),
                            &signers,
                            recent_hash,
                        );
                        if simulate {
                            let ret = simulate_transaction(
                                &rpc_client,
                                &txn,
                                true,
                                CommitmentConfig::confirmed(),
                            )?;
                            println!("{:#?}", ret);
                        } else {
                            let signature = send_txn(&rpc_client, &txn, true)?;
                            println!("{}", signature);
                        }
                    } else {
                        // personal position not exist
                        println!("personal position exist:{:?}", find_position);
                    }
                } else {
                    println!("invalid command: [decrease_liquidity tick_lower_index tick_upper_index liquidity amount_0_min amount_1_min, simulate]");
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
                        raydium_amm_v3::states::TickArrayState::get_array_start_index(
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
                    let mut limit_price = None;
                    if v.len() == 5 {
                        limit_price = Some(v[4].parse::<f64>().unwrap());
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

                    let mut sqrt_price_limit_x64 = None;
                    if limit_price.is_some() {
                        let sqrt_price_x64 = price_to_sqrt_price_x64(
                            limit_price.unwrap(),
                            pool_state.mint_decimals_0,
                            pool_state.mint_decimals_1,
                        );
                        sqrt_price_limit_x64 = Some(sqrt_price_x64);
                    }

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
                    let mut limit_price = None;
                    if v.len() == 5 {
                        limit_price = Some(v[4].parse::<f64>().unwrap());
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

                    let mut sqrt_price_limit_x64 = None;
                    if limit_price.is_some() {
                        let sqrt_price_x64 = price_to_sqrt_price_x64(
                            limit_price.unwrap(),
                            pool_state.mint_decimals_0,
                            pool_state.mint_decimals_1,
                        );
                        sqrt_price_limit_x64 = Some(sqrt_price_x64);
                    }

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
            "x64_to_f" => {
                if v.len() == 2 {
                    let x_64 = v[1].parse::<u128>().unwrap();
                    let f = (x_64 >> fixed_point_64::RESOLUTION) as f64
                        + (x_64 % fixed_point_64::Q64) as f64 / fixed_point_64::Q64 as f64;
                    println!("float:{}", f);
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
            "f_price_to_tick" => {
                if v.len() == 5 {
                    let price = v[1].parse::<f64>().unwrap();
                    let mint_decimals_0 = v[2].parse::<u8>().unwrap();
                    let mint_decimals_1 = v[3].parse::<u8>().unwrap();
                    let tick_spacing = v[4].parse::<u8>().unwrap();
                    let tick_price_x64 =
                        price_to_sqrt_price_x64(price, mint_decimals_0, mint_decimals_1);
                    let tick_index = tick_with_spacing(
                        tick_math::get_tick_at_sqrt_price(tick_price_x64)?,
                        tick_spacing.into(),
                    );
                    println!("tick_index:{}", tick_index);
                } else {
                    println!("f_price_to_tick price mint_decimals_0 mint_decimals_1 tick_spacing")
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
            "decode_instruction" => {
                if v.len() == 2 {
                    let instr_data = v[1];
                    let data = hex::decode(instr_data)?;
                    let mut ix_data: &[u8] = &data;
                    let mut sighash: [u8; 8] = [0; 8];
                    sighash.copy_from_slice(&ix_data[..8]);
                    ix_data = ix_data.get(8..).unwrap();

                    match sighash {
                        [135, 128, 47, 77, 15, 152, 240, 49] => {
                            let ix = raydium_amm_v3::instruction::OpenPosition::deserialize(
                                &mut &ix_data[..],
                            )
                            .map_err(|_| {
                                anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                            })
                            .unwrap();
                            let raydium_amm_v3::instruction::OpenPosition {
                                tick_lower_index,
                                tick_upper_index,
                                tick_array_lower_start_index,
                                tick_array_upper_start_index,
                                liquidity,
                                amount_0_max,
                                amount_1_max,
                            } = ix;
                            println!("tick_lower_index:{}, tick_upper_index:{}, tick_array_lower_start_index:{}, tick_array_upper_start_index:{}, liquidity:{}, amount_0_max{}, amount_1_max{}", tick_lower_index, tick_upper_index, tick_array_lower_start_index, tick_array_upper_start_index, liquidity, amount_0_max, amount_1_max);
                        }
                        [46, 156, 243, 118, 13, 205, 251, 178] => {
                            let ix = raydium_amm_v3::instruction::IncreaseLiquidity::deserialize(
                                &mut &ix_data[..],
                            )
                            .map_err(|_| {
                                anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                            })
                            .unwrap();
                            let raydium_amm_v3::instruction::IncreaseLiquidity {
                                liquidity,
                                amount_0_max,
                                amount_1_max,
                            } = ix;
                            println!(
                                "liquidity:{}, amount_0_max:{}, amount_1_max:{}",
                                liquidity, amount_0_max, amount_1_max
                            );
                        }
                        [160, 38, 208, 111, 104, 91, 44, 1] => {
                            let ix = raydium_amm_v3::instruction::DecreaseLiquidity::deserialize(
                                &mut &ix_data[..],
                            )
                            .map_err(|_| {
                                anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                            })
                            .unwrap();
                            let raydium_amm_v3::instruction::DecreaseLiquidity {
                                liquidity,
                                amount_0_min,
                                amount_1_min,
                            } = ix;
                            println!(
                                "liquidity:{}, amount_0_min:{}, amount_1_min:{}",
                                liquidity, amount_0_min, amount_1_min
                            );
                        }
                        [248, 198, 158, 145, 225, 117, 135, 200] => {
                            let ix =
                                raydium_amm_v3::instruction::Swap::deserialize(&mut &ix_data[..])
                                    .map_err(|_| {
                                        anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                                    })
                                    .unwrap();
                            let raydium_amm_v3::instruction::Swap {
                                amount,
                                other_amount_threshold,
                                sqrt_price_limit_x64,
                                is_base_input,
                            } = ix;
                            println!(
                                "amount:{}, other_amount_threshold:{}, sqrt_price_limit_x64:{}, is_base_input:{}",
                                amount, other_amount_threshold, sqrt_price_limit_x64, is_base_input
                            );
                        }
                        [95, 135, 192, 196, 242, 129, 230, 68] => {
                            let ix = raydium_amm_v3::instruction::InitializeReward::deserialize(
                                &mut &ix_data[..],
                            )
                            .map_err(|_| {
                                anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                            })
                            .unwrap();
                            let raydium_amm_v3::instructions::InitializeRewardParam {
                                open_time,
                                end_time,
                                emissions_per_second_x64,
                            } = ix.param;
                            println!(
                                "open_time:{}, end_time:{}, emissions_per_second_x64:{}",
                                open_time, end_time, emissions_per_second_x64
                            );
                        }
                        _ => {
                            println!("Not decode yet");
                        }
                    }
                }
            }
            "decode_log_event" => {
                if v.len() == 2 {
                    let log_event = v[1];
                    let borsh_bytes = match anchor_lang::__private::base64::decode(&log_event) {
                        Ok(borsh_bytes) => borsh_bytes,
                        _ => {
                            println!("Could not base64 decode log: {}", log_event);
                            return Ok(());
                        }
                    };
                    let mut slice: &[u8] = &borsh_bytes[..];
                    let disc: [u8; 8] = {
                        let mut disc = [0; 8];
                        disc.copy_from_slice(&borsh_bytes[..8]);
                        slice = &slice[8..];
                        disc
                    };
                    match disc {
                        [64, 198, 205, 232, 38, 8, 113, 226] => {
                            let log =
                                raydium_amm_v3::states::SwapEvent::deserialize(&mut &slice[..])
                                    .map_err(|_| {
                                        anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                                    })
                                    .unwrap();
                            let raydium_amm_v3::states::SwapEvent {
                                pool_state,
                                sender,
                                token_account_0,
                                token_account_1,
                                amount_0,
                                amount_1,
                                zero_for_one,
                                sqrt_price_x64,
                                liquidity,
                                tick,
                            } = log;
                            let pool_f_price = sqrt_price_x64_to_price(sqrt_price_x64, 9, 6);
                            println!("pool_state:{}, sender:{}, token_account_0:{}, token_account_1:{}, amount_0:{}, amount_1:{}, zero_for_one:{}, sqrt_price_x64:{}, pool_f_price:{}, liquidity:{}, tick:{}", pool_state, sender, token_account_0, token_account_1, amount_0, amount_1, zero_for_one, sqrt_price_x64, pool_f_price, liquidity, tick);
                        }
                        [58, 222, 86, 58, 68, 50, 85, 56] => {
                            let log = raydium_amm_v3::states::DecreaseLiquidityEvent::deserialize(
                                &mut &slice[..],
                            )
                            .map_err(|_| {
                                anchor_lang::error::ErrorCode::InstructionDidNotDeserialize
                            })
                            .unwrap();
                            let raydium_amm_v3::states::DecreaseLiquidityEvent {
                                position_nft_mint,
                                liquidity,
                                decrease_amount_0,
                                decrease_amount_1,
                                fee_amount_0,
                                fee_amount_1,
                                reward_amounts,
                            } = log;
                            println!("position_nft_mint:{}, liquidity:{}, decrease_amount_0:{}, decrease_amount_1:{}, fee_amount_0:{}, fee_amount_1:{}, reward_amounts:{:?}", position_nft_mint, liquidity, decrease_amount_0, decrease_amount_1,fee_amount_0,fee_amount_1,reward_amounts);
                        }
                        _ => {
                            println!("Not decode yet");
                        }
                    }
                }
            }
            "transfer_reward_owner" => {
                if v.len() != 3 {
                    panic!("invalild args")
                }
                let pool_id = Pubkey::from_str(&v[1]).unwrap();
                let new_owner = Pubkey::from_str(&v[2]).unwrap();
                let transfer_reward_owner_instrs =
                    transfer_reward_owner(&pool_config.clone(), pool_id, new_owner).unwrap();
                // send
                let signers = vec![&payer, &admin];
                let recent_hash = rpc_client.get_latest_blockhash()?;
                let txn = Transaction::new_signed_with_payer(
                    &transfer_reward_owner_instrs,
                    Some(&payer.pubkey()),
                    &signers,
                    recent_hash,
                );
                let signature = send_txn(&rpc_client, &txn, true)?;
                println!("{}", signature);
            }
            _ => {
                println!("command not exist");
            }
        }
    }
}
