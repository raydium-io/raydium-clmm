/// A library for handling Q64.64 fixed point numbers
/// Used in sqrt_price_math.rs and liquidity_amounts.rs

pub const Q64: u128 = (u64::MAX as u128) + 1; // 2^64
pub const RESOLUTION: u8 = 64;
