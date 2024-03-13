use anchor_client::solana_sdk::commitment_config::CommitmentConfig;
use anchor_client::solana_sdk::program_pack::Pack;
use anchor_spl::metadata::Metadata;
use solana_account_decoder::UiDataSliceConfig;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use spl_associated_token_account::{get_associated_token_address};
use switchboard_solana::anchor_client::Client;
use switchboard_solana::{anchor_client::Program, Keypair, Pubkey};
pub use mpl_token_metadata::ID as TOKEN_METADATA_PROGRAM_ID;
use solana_client::rpc_filter::RpcFilterType;
use crate::solana_sdk::commitment_config::CommitmentLevel;
use solana_client::rpc_filter::Memcmp;
use solana_account_decoder::UiAccountEncoding;
use solana_client::rpc_filter::MemcmpEncodedBytes;
pub use switchboard_solana::prelude::*;
pub use solana_client::*;
use std::sync::Arc;
pub use switchboard_solana::get_ixn_discriminator;
pub use switchboard_solana::prelude::*;
use switchboard_solana::sb_error;
use std::str::FromStr;
use switchboard_solana::switchboard_function;
use switchboard_utils;
use switchboard_utils::SbError;
use tokio;

pub struct Holder {
    pub pubkey: Pubkey,
    pub amount: u64,
    pub owner: Pubkey,
}

use ethers::types::I256;

declare_id!("Gyb6RKsLsZa1UCJkCmKYHtEJQF15wF6ZeEqMUSCneh9d");

#[derive(Clone)]
pub struct StakeProgram;

impl anchor_lang::Id for StakeProgram {
    fn id() -> Pubkey {
        Pubkey::from_str("Stake11111111111111111111111111111111111111").unwrap()
    }
}
fn generate_randomness(min: u32, max: u32) -> u32 {
    if min == max {
        return min;
    }
    if min > max {
        return generate_randomness(max, min);
    }

    // We add one so its inclusive [min, max]
    let window = (max + 1) - min;

    let mut bytes: [u8; 4] = [0u8; 4];
    Gramine::read_rand(&mut bytes).expect("gramine failed to generate randomness");
    let raw_result: &[u32] = bytemuck::cast_slice(&bytes[..]);

    (raw_result[0] % window) + min
}


#[switchboard_function]
pub async fn etherprices_oracle_function(
    runner: Arc<FunctionRunner>,
    _params: Vec<u8>,
) -> Result<Vec<Instruction>, SbFunctionError> {
    msg!("etherprices_oracle_function");
    
    // Define the program ID of your deployed Anchor program
    let program_id = raydium_amm_v3::id();
    let keypair = Keypair::new();
    let client = Client::new_with_options(
        Cluster::Custom("https://jarrett-solana-7ba9.mainnet.rpcpool.com/8d890735-edf2-4a75-af84-92f7c9e31718".to_string(), "https://jarrett-solana-7ba9.mainnet.rpcpool.com/8d890735-edf2-4a75-af84-92f7c9e31718".to_string()),
        Arc::new(keypair),
        CommitmentConfig::processed(),
    );
    let program: Program<Arc<Keypair>> =
        client.program(program_id).unwrap();

    let pools = program.async_rpc().get_program_accounts_with_config(
        &raydium_amm_v3::id(),
        solana_client::rpc_config::RpcProgramAccountsConfig {
            filters: None
        ,account_config: RpcAccountInfoConfig {
                min_context_slot: None,
                encoding: Some(solana_account_decoder::UiAccountEncoding::Base64Zstd),
                commitment: Some(CommitmentConfig::processed()),
                data_slice: Some(UiDataSliceConfig {
                    offset: 0,
                    length: 1544 as usize
                }),
            },
            ..RpcProgramAccountsConfig::default()
        },
    ).await.unwrap();
    let mut ixns: Vec<Instruction> = vec![];
    for pool in pools {
        let mut holders: Vec<Holder> = vec![];
        let buf:&mut &[u8] = &mut &pool.1.data[..];
        let parsed_pool: raydium_amm_v3::states::pool::PoolState = raydium_amm_v3::states::pool::PoolState::try_deserialize_unchecked(buf).unwrap();
        let update_authority = pool.0;

        #[allow(deprecated)]
        let filter = RpcFilterType::Memcmp(Memcmp {
            offset: 1, // key
            bytes: MemcmpEncodedBytes::Bytes(update_authority.to_bytes().to_vec()),
            encoding: None,
        });
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![filter]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig {
                    commitment: CommitmentLevel::Confirmed,
                }),
                min_context_slot: None,
            },
            with_context: None,
        };
    
        let mints = program.async_rpc().get_program_accounts_with_config(&TOKEN_METADATA_PROGRAM_ID, config).await.unwrap();
        for mint in mints {
            let buf:&mut &[u8] = &mut &mint.1.data[..];
            let parsed = mpl_token_metadata::accounts::Metadata::deserialize(buf).unwrap();
            println!("parsed: {:?}", parsed);

            let h = program.async_rpc().get_program_accounts_with_config(
                &anchor_spl::token_interface::Token2022::id(),
                solana_client::rpc_config::RpcProgramAccountsConfig {
                    filters: Some(vec![
                        solana_client::rpc_filter::RpcFilterType::Memcmp(solana_client::rpc_filter::Memcmp {
                        offset: 0,
                        bytes: solana_client::rpc_filter::MemcmpEncodedBytes::Binary(mint.0.to_string()),
                        encoding: None
                    })]),account_config: RpcAccountInfoConfig {
                        min_context_slot: None,
                        encoding: Some(solana_account_decoder::UiAccountEncoding::Base64Zstd),
                        commitment: Some(CommitmentConfig::processed()),
                        data_slice: Some(UiDataSliceConfig {
                            offset: 0,
                            length: 165 as usize
                        }),
                    },
                    ..RpcProgramAccountsConfig::default()
                },
            ).await.unwrap();

    println!("holders: {:?}", h.len());
    let mut hs: Vec<Holder> = h
        .into_iter()
        .map(|acc| {
            let buf:&mut &[u8] = &mut &acc.1.data[..];


            let parsed: anchor_spl::token_interface::TokenAccount = anchor_spl::token_interface::TokenAccount::try_deserialize_unchecked(buf).unwrap();
            Holder {
                pubkey: acc.0,
                amount: parsed.amount,
                owner: parsed.owner,
            }
        })
        
        .collect(); 
    holders.extend(hs);
        }
        println!("pooliemcpool hodlers len: {:?}", holders.len());
        let mut total: u32 = 0;
        for holder in &holders {
            total += (holder.amount ) as u32;
        }
        let random_result = generate_randomness(0, total);
        let mut actual_destination = Pubkey::default();
        let mut new_winner_winner_chickum_dinner = Pubkey::default();
        let mut old_new_winner_winner_chickum_dinner = Pubkey::default();
        for holder in &holders {
            total -= (holder.amount ) as u32;
            if total < random_result || total == 0 {
                // get associated token account for token_2022
                actual_destination = holder.pubkey;
                old_new_winner_winner_chickum_dinner = new_winner_winner_chickum_dinner;
                new_winner_winner_chickum_dinner = holder.owner;
                break;
            }
        }
        println!("actual_destination: {:?}", actual_destination);
        println!("new_winner_winner_chickum_dinner: {:?}", new_winner_winner_chickum_dinner);
        let ixn = Instruction {
            program_id: program_id,
            accounts: vec![
                AccountMeta {
                    pubkey: new_winner_winner_chickum_dinner,
                    is_signer: false,
                    is_writable: false
                },
                AccountMeta {
                    pubkey: old_new_winner_winner_chickum_dinner,
                    is_signer: false,
                    is_writable: true
                },
            AccountMeta {
                pubkey: runner.function,
                is_signer: false,
                is_writable: false,
            },
            AccountMeta {
                pubkey: runner.signer,
                is_signer: true,
                is_writable: false,
            },
                ],
                data: [
                    get_ixn_discriminator("chickum_dinners").to_vec(),
                ]
                .concat(),
            };
            ixns.push(ixn);
    }
            Ok(ixns)
   
}

#[sb_error]
pub enum Error {
    InvalidResult,
}