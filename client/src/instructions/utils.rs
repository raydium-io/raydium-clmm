use anchor_lang::AccountDeserialize;
use anyhow::Result;
use raydium_amm_v3::libraries::fixed_point_64;
use solana_sdk::account::Account;

pub fn deserialize_anchor_account<T: AccountDeserialize>(account: Account) -> Result<T> {
    let mut data: &[u8] = &account.data;
    T::try_deserialize(&mut data).map_err(Into::into)
}

pub const Q_RATIO: f64 = 1.0001;

pub fn tick_to_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick)
}

pub fn price_to_tick(price: f64) -> i32 {
    price.log(Q_RATIO) as i32
}

pub fn tick_to_sqrt_price(tick: i32) -> f64 {
    Q_RATIO.powi(tick).sqrt()
}

pub fn tick_with_spacing(tick: i32, tick_spacing: i32) -> i32 {
    let mut compressed = tick / tick_spacing;
    if tick < 0 && tick % tick_spacing != 0 {
        compressed -= 1; // round towards negative infinity
    }
    compressed * tick_spacing
}

pub fn multipler(decimals: u8) -> f64 {
    (10_i32).checked_pow(decimals.try_into().unwrap()).unwrap() as f64
}

pub fn price_to_x64(price: f64) -> u128 {
    (price * fixed_point_64::Q64 as f64) as u128
}

pub fn from_x64_price(price: u128) -> f64 {
    price as f64 / fixed_point_64::Q64 as f64
}

pub fn price_to_sqrt_price_x64(price: f64, decimals_0: u8, decimals_1: u8) -> u128 {
    let price_with_decimals = price * multipler(decimals_1) / multipler(decimals_0);
    price_to_x64(price_with_decimals.sqrt())
}

pub fn sqrt_price_x64_to_price(price: u128, decimals_0: u8, decimals_1: u8) -> f64 {
    from_x64_price(price).powi(2) * multipler(decimals_0) / multipler(decimals_1)
}
