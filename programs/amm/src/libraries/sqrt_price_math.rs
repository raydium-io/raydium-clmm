use super::full_math::MulDiv;
use super::unsafe_math::UnsafeMathTrait;
use super::{fixed_point_64, U256};

/// Gets the next sqrt price √P' given a delta of token_0
///
/// Always round up because
/// 1. In the exact output case, token 0 supply decreases leading to price increase.
/// Move price up so that exact output is met.
/// 2. In the exact input case, token 0 supply increases leading to price decrease.
/// Do not round down to minimize price impact. We only need to meet input
/// change and not guarantee exact output.
///
/// Use function for exact input or exact output swaps for token 0
///
/// # Formula
///
/// * `√P' = √P * L / (L + Δx * √P)`
/// * If Δx * √P overflows, use alternate form `√P' = L / (L/√P + Δx)`
///
/// # Proof
///
/// For constant y,
/// √P * L = y
/// √P' * L' = √P * L
/// √P' = √P * L / L'
/// √P' = √P * L / L'
/// √P' = √P * L / (L + Δx*√P)
///
pub fn get_next_sqrt_price_from_amount_0_rounding_up(
    sqrt_price_x64: u128,
    liquidity: u128,
    amount: u64,
    add: bool,
) -> u128 {
    if amount == 0 {
        return sqrt_price_x64;
    };
    let numerator_1 = (U256::from(liquidity)) << fixed_point_64::RESOLUTION;

    if add {
        if let Some(product) = U256::from(amount).checked_mul(U256::from(sqrt_price_x64)) {
            let denominator = numerator_1 + U256::from(product);
            if denominator >= numerator_1 {
                return numerator_1
                    .mul_div_ceil(U256::from(sqrt_price_x64), denominator)
                    .unwrap()
                    .as_u128();
            };
        }

        U256::div_rounding_up(
            numerator_1,
            (numerator_1 / U256::from(sqrt_price_x64))
                .checked_add(U256::from(amount))
                .unwrap(),
        )
        .as_u128()
    } else {
        let product = U256::from(
            U256::from(amount)
                .checked_mul(U256::from(sqrt_price_x64))
                .unwrap(),
        );
        let denominator = numerator_1.checked_sub(product).unwrap();
        numerator_1
            .mul_div_ceil(U256::from(sqrt_price_x64), denominator)
            .unwrap()
            .as_u128()
    }
}

/// Gets the next sqrt price given a delta of token_1
///
/// Always round down because
/// 1. In the exact output case, token 1 supply decreases leading to price decrease.
/// Move price down by rounding down so that exact output of token 0 is met.
/// 2. In the exact input case, token 1 supply increases leading to price increase.
/// Do not round down to minimize price impact. We only need to meet input
/// change and not gurantee exact output for token 0.
///
///
/// # Formula
///
/// * `√P' = √P + Δy / L`
///
pub fn get_next_sqrt_price_from_amount_1_rounding_down(
    sqrt_price_x64: u128,
    liquidity: u128,
    amount: u64,
    add: bool,
) -> u128 {
    if add {
        let quotient = U256::from(u128::from(amount) << fixed_point_64::RESOLUTION) / liquidity;
        sqrt_price_x64.checked_add(quotient.as_u128()).unwrap()
    } else {
        let quotient = U256::div_rounding_up(
            U256::from(u128::from(amount) << fixed_point_64::RESOLUTION),
            U256::from(liquidity),
        );
        sqrt_price_x64.checked_sub(quotient.as_u128()).unwrap()
    }
}

/// Gets the next sqrt price given an input amount of token_0 or token_1
/// Throws if price or liquidity are 0, or if the next price is out of bounds
pub fn get_next_sqrt_price_from_input(
    sqrt_price_x64: u128,
    liquidity: u128,
    amount_in: u64,
    zero_for_one: bool,
) -> u128 {
    assert!(sqrt_price_x64 > 0);
    assert!(liquidity > 0);

    // round to make sure that we don't pass the target price
    if zero_for_one {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_price_x64, liquidity, amount_in, true)
    } else {
        get_next_sqrt_price_from_amount_1_rounding_down(sqrt_price_x64, liquidity, amount_in, true)
    }
}

/// Gets the next sqrt price given an output amount of token0 or token1
///
/// Throws if price or liquidity are 0 or the next price is out of bounds
///
pub fn get_next_sqrt_price_from_output(
    sqrt_price_x64: u128,
    liquidity: u128,
    amount_out: u64,
    zero_for_one: bool,
) -> u128 {
    assert!(sqrt_price_x64 > 0);
    assert!(liquidity > 0);

    if zero_for_one {
        get_next_sqrt_price_from_amount_1_rounding_down(
            sqrt_price_x64,
            liquidity,
            amount_out,
            false,
        )
    } else {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_price_x64, liquidity, amount_out, false)
    }
}
