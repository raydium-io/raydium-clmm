/// A library for handling Q32.32 fixed point numbers
/// Used in sqrt_price_math.rs and position.rs

pub const Q32: u64 = (u32::MAX as u64) + 1; // 2^32
pub const RESOLUTION: u8 = 32;
