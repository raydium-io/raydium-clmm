use anchor_client::ClientError;
use anchor_lang::prelude::Pubkey;
use anchor_lang::Discriminator;
use anyhow::Result;
use colorful::Color;
use colorful::Colorful;
use raydium_amm_v3::instruction;
use raydium_amm_v3::instructions::*;
use raydium_amm_v3::states::*;
use regex::Regex;
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiTransactionStatusMeta,
};

const PROGRAM_LOG: &str = "Program log: ";
const PROGRAM_DATA: &str = "Program data: ";

pub enum InstructionDecodeType {
    BaseHex,
    Base64,
    Base58,
}

pub fn parse_program_event(
    self_program_str: &str,
    meta: Option<UiTransactionStatusMeta>,
) -> Result<(), ClientError> {
    let logs: Vec<String> = if let Some(meta_data) = meta {
        let log_messages = if let OptionSerializer::Some(log_messages) = meta_data.log_messages {
            log_messages
        } else {
            Vec::new()
        };
        log_messages
    } else {
        Vec::new()
    };
    let mut logs = &logs[..];
    if !logs.is_empty() {
        if let Ok(mut execution) = Execution::new(&mut logs) {
            for l in logs {
                let (new_program, did_pop) =
                    if !execution.is_empty() && self_program_str == execution.program() {
                        handle_program_log(self_program_str, &l, true).unwrap_or_else(|e| {
                            println!("Unable to parse log: {e}");
                            std::process::exit(1);
                        })
                    } else {
                        let (program, did_pop) = handle_system_log(self_program_str, l);
                        (program, did_pop)
                    };
                // Switch program context on CPI.
                if let Some(new_program) = new_program {
                    execution.push(new_program);
                }
                // Program returned.
                if did_pop {
                    execution.pop();
                }
            }
        }
    } else {
        println!("log is empty");
    }
    Ok(())
}

struct Execution {
    stack: Vec<String>,
}

impl Execution {
    pub fn new(logs: &mut &[String]) -> Result<Self, ClientError> {
        let l = &logs[0];
        *logs = &logs[1..];

        let re = Regex::new(r"^Program (.*) invoke.*$").unwrap();
        let c = re
            .captures(l)
            .ok_or_else(|| ClientError::LogParseError(l.to_string()))?;
        let program = c
            .get(1)
            .ok_or_else(|| ClientError::LogParseError(l.to_string()))?
            .as_str()
            .to_string();
        Ok(Self {
            stack: vec![program],
        })
    }

    pub fn program(&self) -> String {
        assert!(!self.stack.is_empty());
        self.stack[self.stack.len() - 1].clone()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn push(&mut self, new_program: String) {
        self.stack.push(new_program);
    }

    pub fn pop(&mut self) {
        assert!(!self.stack.is_empty());
        self.stack.pop().unwrap();
    }
}

pub fn handle_program_log(
    self_program_str: &str,
    l: &str,
    with_prefix: bool,
) -> Result<(Option<String>, bool), ClientError> {
    // Log emitted from the current program.
    if let Some(log) = if with_prefix {
        l.strip_prefix(PROGRAM_LOG)
            .or_else(|| l.strip_prefix(PROGRAM_DATA))
    } else {
        Some(l)
    } {
        if l.starts_with(&format!("Program log:")) {
            // not log event
            return Ok((None, false));
        }
        let borsh_bytes = match anchor_lang::__private::base64::decode(log) {
            Ok(borsh_bytes) => borsh_bytes,
            _ => {
                println!("Could not base64 decode log: {}", log);
                return Ok((None, false));
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
            ConfigChangeEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<ConfigChangeEvent>(&mut slice)?);
            }
            CollectPersonalFeeEvent::DISCRIMINATOR => {
                println!(
                    "{:#?}",
                    decode_event::<CollectPersonalFeeEvent>(&mut slice)?
                );
            }
            CollectProtocolFeeEvent::DISCRIMINATOR => {
                println!(
                    "{:#?}",
                    decode_event::<CollectProtocolFeeEvent>(&mut slice)?
                );
            }
            CreatePersonalPositionEvent::DISCRIMINATOR => {
                println!(
                    "{:#?}",
                    decode_event::<CreatePersonalPositionEvent>(&mut slice)?
                );
            }
            DecreaseLiquidityEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<DecreaseLiquidityEvent>(&mut slice)?);
            }
            IncreaseLiquidityEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<IncreaseLiquidityEvent>(&mut slice)?);
            }
            LiquidityCalculateEvent::DISCRIMINATOR => {
                println!(
                    "{:#?}",
                    decode_event::<LiquidityCalculateEvent>(&mut slice)?
                );
            }
            LiquidityChangeEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<LiquidityChangeEvent>(&mut slice)?);
            }
            // PriceChangeEvent::DISCRIMINATOR => {
            //     println!("{:#?}", decode_event::<PriceChangeEvent>(&mut slice)?);
            // }
            SwapEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<SwapEvent>(&mut slice)?);
            }
            PoolCreatedEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<PoolCreatedEvent>(&mut slice)?);
            }
            _ => {
                println!("unknow event: {}", l);
            }
        }
        return Ok((None, false));
    } else {
        let (program, did_pop) = handle_system_log(self_program_str, l);
        return Ok((program, did_pop));
    }
}

fn handle_system_log(this_program_str: &str, log: &str) -> (Option<String>, bool) {
    if log.starts_with(&format!("Program {this_program_str} invoke")) {
        (Some(this_program_str.to_string()), false)
    } else if log.contains("invoke") {
        (Some("cpi".to_string()), false) // Any string will do.
    } else {
        let re = Regex::new(r"^Program (.*) success*$").unwrap();
        if re.is_match(log) {
            (None, true)
        } else {
            (None, false)
        }
    }
}

fn decode_event<T: anchor_lang::Event + anchor_lang::AnchorDeserialize>(
    slice: &mut &[u8],
) -> Result<T, ClientError> {
    let event: T = anchor_lang::AnchorDeserialize::deserialize(slice)
        .map_err(|e| ClientError::LogParseError(e.to_string()))?;
    Ok(event)
}

pub fn parse_program_instruction(
    self_program_str: &str,
    encoded_transaction: EncodedTransaction,
    meta: Option<UiTransactionStatusMeta>,
) -> Result<(), ClientError> {
    let ui_raw_msg = match encoded_transaction {
        solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
            let ui_message = ui_tx.message;
            // println!("{:#?}", ui_message);
            match ui_message {
                solana_transaction_status::UiMessage::Raw(ui_raw_msg) => ui_raw_msg,
                _ => solana_transaction_status::UiRawMessage {
                    header: solana_sdk::message::MessageHeader::default(),
                    account_keys: Vec::new(),
                    recent_blockhash: "".to_string(),
                    instructions: Vec::new(),
                    address_table_lookups: None,
                },
            }
        }
        _ => solana_transaction_status::UiRawMessage {
            header: solana_sdk::message::MessageHeader::default(),
            account_keys: Vec::new(),
            recent_blockhash: "".to_string(),
            instructions: Vec::new(),
            address_table_lookups: None,
        },
    };
    // append lookup table keys if necessary
    if meta.is_some() {
        let mut account_keys = ui_raw_msg.account_keys;
        let meta = meta.clone().unwrap();
        match meta.loaded_addresses {
            OptionSerializer::Some(addresses) => {
                let mut writeable_address = addresses.writable;
                let mut readonly_address = addresses.readonly;
                account_keys.append(&mut writeable_address);
                account_keys.append(&mut readonly_address);
            }
            _ => {}
        }
        let program_index = account_keys
            .iter()
            .position(|r| r == self_program_str)
            .unwrap();
        // println!("{}", program_index);
        // println!("{:#?}", account_keys);
        for (i, ui_compiled_instruction) in ui_raw_msg.instructions.iter().enumerate() {
            if (ui_compiled_instruction.program_id_index as usize) == program_index {
                let out_put = format!("instruction #{}", i + 1);
                println!("{}", out_put.gradient(Color::Green));
                handle_program_instruction(
                    &ui_compiled_instruction.data,
                    InstructionDecodeType::Base58,
                )?;
            }
        }

        match meta.inner_instructions {
            OptionSerializer::Some(inner_instructions) => {
                for inner in inner_instructions {
                    for (i, instruction) in inner.instructions.iter().enumerate() {
                        match instruction {
                            solana_transaction_status::UiInstruction::Compiled(
                                ui_compiled_instruction,
                            ) => {
                                if (ui_compiled_instruction.program_id_index as usize)
                                    == program_index
                                {
                                    let out_put =
                                        format!("inner_instruction #{}.{}", inner.index + 1, i + 1);
                                    println!("{}", out_put.gradient(Color::Green));
                                    handle_program_instruction(
                                        &ui_compiled_instruction.data,
                                        InstructionDecodeType::Base58,
                                    )?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn handle_program_instruction(
    instr_data: &str,
    decode_type: InstructionDecodeType,
) -> Result<(), ClientError> {
    let data;
    match decode_type {
        InstructionDecodeType::BaseHex => {
            data = hex::decode(instr_data).unwrap();
        }
        InstructionDecodeType::Base64 => {
            let borsh_bytes = match anchor_lang::__private::base64::decode(instr_data) {
                Ok(borsh_bytes) => borsh_bytes,
                _ => {
                    println!("Could not base64 decode instruction: {}", instr_data);
                    return Ok(());
                }
            };
            data = borsh_bytes;
        }
        InstructionDecodeType::Base58 => {
            let borsh_bytes = match bs58::decode(instr_data).into_vec() {
                Ok(borsh_bytes) => borsh_bytes,
                _ => {
                    println!("Could not base58 decode instruction: {}", instr_data);
                    return Ok(());
                }
            };
            data = borsh_bytes;
        }
    }

    let mut ix_data: &[u8] = &data[..];
    let disc: [u8; 8] = {
        let mut disc = [0; 8];
        disc.copy_from_slice(&data[..8]);
        ix_data = &ix_data[8..];
        disc
    };
    // println!("{:?}", disc);

    match disc {
        instruction::CreateAmmConfig::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CreateAmmConfig>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CreateAmmConfig {
                pub index: u16,
                pub tick_spacing: u16,
                pub trade_fee_rate: u32,
                pub protocol_fee_rate: u32,
                pub fund_fee_rate: u32,
            }
            impl From<instruction::CreateAmmConfig> for CreateAmmConfig {
                fn from(instr: instruction::CreateAmmConfig) -> CreateAmmConfig {
                    CreateAmmConfig {
                        index: instr.index,
                        tick_spacing: instr.tick_spacing,
                        trade_fee_rate: instr.trade_fee_rate,
                        protocol_fee_rate: instr.protocol_fee_rate,
                        fund_fee_rate: instr.fund_fee_rate,
                    }
                }
            }
            println!("{:#?}", CreateAmmConfig::from(ix));
        }
        instruction::UpdateAmmConfig::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::UpdateAmmConfig>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdateAmmConfig {
                pub param: u8,
                pub value: u32,
            }
            impl From<instruction::UpdateAmmConfig> for UpdateAmmConfig {
                fn from(instr: instruction::UpdateAmmConfig) -> UpdateAmmConfig {
                    UpdateAmmConfig {
                        param: instr.param,
                        value: instr.value,
                    }
                }
            }
            println!("{:#?}", UpdateAmmConfig::from(ix));
        }
        instruction::CreatePool::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CreatePool>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CreatePool {
                pub sqrt_price_x64: u128,
                pub open_time: u64,
            }
            impl From<instruction::CreatePool> for CreatePool {
                fn from(instr: instruction::CreatePool) -> CreatePool {
                    CreatePool {
                        sqrt_price_x64: instr.sqrt_price_x64,
                        open_time: instr.open_time,
                    }
                }
            }
            println!("{:#?}", CreatePool::from(ix));
        }
        instruction::UpdatePoolStatus::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::UpdatePoolStatus>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdatePoolStatus {
                pub status: u8,
            }
            impl From<instruction::UpdatePoolStatus> for UpdatePoolStatus {
                fn from(instr: instruction::UpdatePoolStatus) -> UpdatePoolStatus {
                    UpdatePoolStatus {
                        status: instr.status,
                    }
                }
            }
            println!("{:#?}", UpdatePoolStatus::from(ix));
        }
        instruction::CreateOperationAccount::DISCRIMINATOR => {
            let ix =
                decode_instruction::<instruction::CreateOperationAccount>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CreateOperationAccount;
            impl From<instruction::CreateOperationAccount> for CreateOperationAccount {
                fn from(_instr: instruction::CreateOperationAccount) -> CreateOperationAccount {
                    CreateOperationAccount
                }
            }
            println!("{:#?}", CreateOperationAccount::from(ix));
        }
        instruction::UpdateOperationAccount::DISCRIMINATOR => {
            let ix =
                decode_instruction::<instruction::UpdateOperationAccount>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdateOperationAccount {
                pub param: u8,
                pub keys: Vec<Pubkey>,
            }
            impl From<instruction::UpdateOperationAccount> for UpdateOperationAccount {
                fn from(instr: instruction::UpdateOperationAccount) -> UpdateOperationAccount {
                    UpdateOperationAccount {
                        param: instr.param,
                        keys: instr.keys,
                    }
                }
            }
            println!("{:#?}", UpdateOperationAccount::from(ix));
        }
        instruction::TransferRewardOwner::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::TransferRewardOwner>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct TransferRewardOwner {
                pub new_owner: Pubkey,
            }
            impl From<instruction::TransferRewardOwner> for TransferRewardOwner {
                fn from(instr: instruction::TransferRewardOwner) -> TransferRewardOwner {
                    TransferRewardOwner {
                        new_owner: instr.new_owner,
                    }
                }
            }
            println!("{:#?}", TransferRewardOwner::from(ix));
        }
        instruction::InitializeReward::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::InitializeReward>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct InitializeReward {
                pub param: InitializeRewardParam,
            }
            impl From<instruction::InitializeReward> for InitializeReward {
                fn from(instr: instruction::InitializeReward) -> InitializeReward {
                    InitializeReward { param: instr.param }
                }
            }
            println!("{:#?}", InitializeReward::from(ix));
        }
        instruction::CollectRemainingRewards::DISCRIMINATOR => {
            let ix =
                decode_instruction::<instruction::CollectRemainingRewards>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CollectRemainingRewards {
                pub reward_index: u8,
            }
            impl From<instruction::CollectRemainingRewards> for CollectRemainingRewards {
                fn from(instr: instruction::CollectRemainingRewards) -> CollectRemainingRewards {
                    CollectRemainingRewards {
                        reward_index: instr.reward_index,
                    }
                }
            }
            println!("{:#?}", CollectRemainingRewards::from(ix));
        }
        instruction::UpdateRewardInfos::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::UpdateRewardInfos>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdateRewardInfos;
            impl From<instruction::UpdateRewardInfos> for UpdateRewardInfos {
                fn from(_instr: instruction::UpdateRewardInfos) -> UpdateRewardInfos {
                    UpdateRewardInfos
                }
            }
            println!("{:#?}", UpdateRewardInfos::from(ix));
        }
        instruction::SetRewardParams::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::SetRewardParams>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct SetRewardParams {
                pub reward_index: u8,
                pub emissions_per_second_x64: u128,
                pub open_time: u64,
                pub end_time: u64,
            }
            impl From<instruction::SetRewardParams> for SetRewardParams {
                fn from(instr: instruction::SetRewardParams) -> SetRewardParams {
                    SetRewardParams {
                        reward_index: instr.reward_index,
                        emissions_per_second_x64: instr.emissions_per_second_x64,
                        open_time: instr.open_time,
                        end_time: instr.end_time,
                    }
                }
            }
            println!("{:#?}", SetRewardParams::from(ix));
        }
        instruction::CollectProtocolFee::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CollectProtocolFee>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CollectProtocolFee {
                pub amount_0_requested: u64,
                pub amount_1_requested: u64,
            }
            impl From<instruction::CollectProtocolFee> for CollectProtocolFee {
                fn from(instr: instruction::CollectProtocolFee) -> CollectProtocolFee {
                    CollectProtocolFee {
                        amount_0_requested: instr.amount_0_requested,
                        amount_1_requested: instr.amount_1_requested,
                    }
                }
            }
            println!("{:#?}", CollectProtocolFee::from(ix));
        }
        instruction::CollectFundFee::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CollectFundFee>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CollectFundFee {
                pub amount_0_requested: u64,
                pub amount_1_requested: u64,
            }
            impl From<instruction::CollectFundFee> for CollectFundFee {
                fn from(instr: instruction::CollectFundFee) -> CollectFundFee {
                    CollectFundFee {
                        amount_0_requested: instr.amount_0_requested,
                        amount_1_requested: instr.amount_1_requested,
                    }
                }
            }
            println!("{:#?}", CollectFundFee::from(ix));
        }
        instruction::OpenPosition::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::OpenPosition>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct OpenPosition {
                pub tick_lower_index: i32,
                pub tick_upper_index: i32,
                pub tick_array_lower_start_index: i32,
                pub tick_array_upper_start_index: i32,
                pub liquidity: u128,
                pub amount_0_max: u64,
                pub amount_1_max: u64,
            }
            impl From<instruction::OpenPosition> for OpenPosition {
                fn from(instr: instruction::OpenPosition) -> OpenPosition {
                    OpenPosition {
                        tick_lower_index: instr.tick_lower_index,
                        tick_upper_index: instr.tick_upper_index,
                        tick_array_lower_start_index: instr.tick_array_lower_start_index,
                        tick_array_upper_start_index: instr.tick_array_upper_start_index,
                        liquidity: instr.liquidity,
                        amount_0_max: instr.amount_0_max,
                        amount_1_max: instr.amount_1_max,
                    }
                }
            }
            println!("{:#?}", OpenPosition::from(ix));
        }
        instruction::OpenPositionV2::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::OpenPositionV2>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct OpenPositionV2 {
                pub tick_lower_index: i32,
                pub tick_upper_index: i32,
                pub tick_array_lower_start_index: i32,
                pub tick_array_upper_start_index: i32,
                pub liquidity: u128,
                pub amount_0_max: u64,
                pub amount_1_max: u64,
                pub base_flag: Option<bool>,
                pub with_metadata: bool,
            }
            impl From<instruction::OpenPositionV2> for OpenPositionV2 {
                fn from(instr: instruction::OpenPositionV2) -> OpenPositionV2 {
                    OpenPositionV2 {
                        tick_lower_index: instr.tick_lower_index,
                        tick_upper_index: instr.tick_upper_index,
                        tick_array_lower_start_index: instr.tick_array_lower_start_index,
                        tick_array_upper_start_index: instr.tick_array_upper_start_index,
                        liquidity: instr.liquidity,
                        amount_0_max: instr.amount_0_max,
                        amount_1_max: instr.amount_1_max,
                        base_flag: instr.base_flag,
                        with_metadata: instr.with_metadata,
                    }
                }
            }
            println!("{:#?}", OpenPositionV2::from(ix));
        }
        instruction::ClosePosition::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::ClosePosition>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct ClosePosition;
            impl From<instruction::ClosePosition> for ClosePosition {
                fn from(_instr: instruction::ClosePosition) -> ClosePosition {
                    ClosePosition
                }
            }
            println!("{:#?}", ClosePosition::from(ix));
        }
        instruction::IncreaseLiquidity::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::IncreaseLiquidity>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct IncreaseLiquidity {
                pub liquidity: u128,
                pub amount_0_max: u64,
                pub amount_1_max: u64,
            }
            impl From<instruction::IncreaseLiquidity> for IncreaseLiquidity {
                fn from(instr: instruction::IncreaseLiquidity) -> IncreaseLiquidity {
                    IncreaseLiquidity {
                        liquidity: instr.liquidity,
                        amount_0_max: instr.amount_0_max,
                        amount_1_max: instr.amount_1_max,
                    }
                }
            }
            println!("{:#?}", IncreaseLiquidity::from(ix));
        }
        instruction::IncreaseLiquidityV2::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::IncreaseLiquidityV2>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct IncreaseLiquidityV2 {
                pub liquidity: u128,
                pub amount_0_max: u64,
                pub amount_1_max: u64,
                pub base_flag: Option<bool>,
            }
            impl From<instruction::IncreaseLiquidityV2> for IncreaseLiquidityV2 {
                fn from(instr: instruction::IncreaseLiquidityV2) -> IncreaseLiquidityV2 {
                    IncreaseLiquidityV2 {
                        liquidity: instr.liquidity,
                        amount_0_max: instr.amount_0_max,
                        amount_1_max: instr.amount_1_max,
                        base_flag: instr.base_flag,
                    }
                }
            }
            println!("{:#?}", IncreaseLiquidityV2::from(ix));
        }
        instruction::DecreaseLiquidity::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::DecreaseLiquidity>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct DecreaseLiquidity {
                pub liquidity: u128,
                pub amount_0_min: u64,
                pub amount_1_min: u64,
            }
            impl From<instruction::DecreaseLiquidity> for DecreaseLiquidity {
                fn from(instr: instruction::DecreaseLiquidity) -> DecreaseLiquidity {
                    DecreaseLiquidity {
                        liquidity: instr.liquidity,
                        amount_0_min: instr.amount_0_min,
                        amount_1_min: instr.amount_1_min,
                    }
                }
            }
            println!("{:#?}", DecreaseLiquidity::from(ix));
        }
        instruction::DecreaseLiquidityV2::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::DecreaseLiquidityV2>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct DecreaseLiquidityV2 {
                pub liquidity: u128,
                pub amount_0_min: u64,
                pub amount_1_min: u64,
            }
            impl From<instruction::DecreaseLiquidityV2> for DecreaseLiquidityV2 {
                fn from(instr: instruction::DecreaseLiquidityV2) -> DecreaseLiquidityV2 {
                    DecreaseLiquidityV2 {
                        liquidity: instr.liquidity,
                        amount_0_min: instr.amount_0_min,
                        amount_1_min: instr.amount_1_min,
                    }
                }
            }
            println!("{:#?}", DecreaseLiquidityV2::from(ix));
        }
        instruction::Swap::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::Swap>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct Swap {
                pub amount: u64,
                pub other_amount_threshold: u64,
                pub sqrt_price_limit_x64: u128,
                pub is_base_input: bool,
            }
            impl From<instruction::Swap> for Swap {
                fn from(instr: instruction::Swap) -> Swap {
                    Swap {
                        amount: instr.amount,
                        other_amount_threshold: instr.other_amount_threshold,
                        sqrt_price_limit_x64: instr.sqrt_price_limit_x64,
                        is_base_input: instr.is_base_input,
                    }
                }
            }
            println!("{:#?}", Swap::from(ix));
        }
        instruction::SwapV2::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::SwapV2>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct SwapV2 {
                pub amount: u64,
                pub other_amount_threshold: u64,
                pub sqrt_price_limit_x64: u128,
                pub is_base_input: bool,
            }
            impl From<instruction::SwapV2> for SwapV2 {
                fn from(instr: instruction::SwapV2) -> SwapV2 {
                    SwapV2 {
                        amount: instr.amount,
                        other_amount_threshold: instr.other_amount_threshold,
                        sqrt_price_limit_x64: instr.sqrt_price_limit_x64,
                        is_base_input: instr.is_base_input,
                    }
                }
            }
            println!("{:#?}", SwapV2::from(ix));
        }
        instruction::SwapRouterBaseIn::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::SwapRouterBaseIn>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct SwapRouterBaseIn {
                pub amount_in: u64,
                pub amount_out_minimum: u64,
            }
            impl From<instruction::SwapRouterBaseIn> for SwapRouterBaseIn {
                fn from(instr: instruction::SwapRouterBaseIn) -> SwapRouterBaseIn {
                    SwapRouterBaseIn {
                        amount_in: instr.amount_in,
                        amount_out_minimum: instr.amount_out_minimum,
                    }
                }
            }
            println!("{:#?}", SwapRouterBaseIn::from(ix));
        }
        _ => {
            println!("unknow instruction: {}", instr_data);
        }
    }
    Ok(())
}

fn decode_instruction<T: anchor_lang::AnchorDeserialize>(
    slice: &mut &[u8],
) -> Result<T, anchor_lang::error::ErrorCode> {
    let instruction: T = anchor_lang::AnchorDeserialize::deserialize(slice)
        .map_err(|_| anchor_lang::error::ErrorCode::InstructionDidNotDeserialize)?;
    Ok(instruction)
}
