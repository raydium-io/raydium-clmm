/// Helper functions for unit tests

#[cfg(test)]

/// Get sqrt current price from reserves of token_1 and token_0
///
/// Where token_0 is base (ETH) and token_1 is quote (USDC)
///
/// # Formula
/// `P = reserve_1 / reserve_0`
///
pub fn encode_price_sqrt_x32(reserve_1: u64, reserve_0: u64) -> u64 {
    ((reserve_1 as f64 / reserve_0 as f64).sqrt() * u64::pow(2, 32) as f64).round() as u64
}

/// Obtain liquidity from virtual reserves of token_1 and token_0
///
pub fn encode_liquidity(reserve_1: u64, reserve_0: u64) -> u64 {
    (reserve_1 as f64 * reserve_0 as f64).sqrt().round() as u64
}
