///! Math library for liquidity
///
use crate::error::ErrorCode;
use anchor_lang::require;

/// Add a signed liquidity delta to liquidity and revert if it overflows or underflows
///
/// # Arguments
///
/// * `x` - The liquidity (L) before change
/// * `y` - The delta (ΔL) by which liquidity should be changed
///
pub fn add_delta(x: u128, y: i128) -> Result<u128, anchor_lang::error::Error> {
    let z: u128;
    if y < 0 {
        z = x - (-y as u128);
        require!(z < x, ErrorCode::LiquiditySubValueErr);
    } else {
        z = x + (y as u128);
        require!(z >= x, ErrorCode::LiquidityAddValueErr);
    }

    Ok(z)
}
