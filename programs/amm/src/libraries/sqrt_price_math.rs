///! Helper functions to find price changes for change in token supply and vice versa
use super::big_num::U128;
use super::fixed_point_64;
use super::full_math::MulDiv;
use super::unsafe_math::UnsafeMathTrait;
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
/// # Arguments
///
/// * `sqrt_p_x64` - The starting price `√P`, i.e., before accounting for the token_1 delta,
/// where P is `token_1_supply / token_0_supply`
/// * `liquidity` - The amount of usable liquidity L
/// * `amount` - Delta of token 0 (Δx) to add or remove from virtual reserves
/// * `add` - Whether to add or remove the amount of token_0
///
pub fn get_next_sqrt_price_from_amount_0_rounding_up(
    sqrt_p_x64: u128,
    liquidity: u128,
    amount: u64,
    add: bool,
) -> u128 {
    // we short circuit amount == 0 because the result is otherwise not
    // guaranteed to equal the input price
    if amount == 0 {
        return sqrt_p_x64;
    };
    let numerator_1 = (U128::from(liquidity)) << fixed_point_64::RESOLUTION; // U32.32

    if add {
        // Used native overflow check instead of the `a * b / b == a` Solidity method
        // https://stackoverflow.com/q/70143451/7721443

        if let Some(product) = U128::from(amount).checked_mul(U128::from(sqrt_p_x64)) {
            let denominator = numerator_1 + U128::from(product);
            if denominator >= numerator_1 {
                return numerator_1
                    .mul_div_ceil(U128::from(sqrt_p_x64), denominator)
                    .unwrap()
                    .as_u128();
            };
        }
        // Alternate form if overflow - `√P' = L / (L/√P + Δx)`

        U128::div_rounding_up(
            numerator_1,
            (numerator_1 / U128::from(sqrt_p_x64))
                .checked_add(U128::from(amount))
                .unwrap(),
        )
        .as_u128()
    } else {
        // if the product overflows, we know the denominator underflows
        // in addition, we must check that the denominator does not underflow
        // assert!(product / amount == sqrt_p_x64 && numerator_1 > product);
        let product = U128::from(
            U128::from(amount)
                .checked_mul(U128::from(sqrt_p_x64))
                .unwrap(),
        );
        assert!(numerator_1 > product);

        let denominator = numerator_1 - product;
        numerator_1
            .mul_div_ceil(U128::from(sqrt_p_x64), denominator)
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
/// # Arguments
///
/// * `sqrt_p_x64` - The starting price `√P`, i.e., before accounting for the token_1 delta
/// * `liquidity` - The amount of usable liquidity L
/// * `amount` - Delta of token 1 (Δy) to add or remove from virtual reserves
/// * `add` - Whether to add or remove the amount of token_1
///
pub fn get_next_sqrt_price_from_amount_1_rounding_down(
    sqrt_p_x64: u128,
    liquidity: u128,
    amount: u64,
    add: bool,
) -> u128 {
    // if we are adding (subtracting), rounding down requires rounding the quotient down (up)
    // in both cases, avoid a mul_div for most inputs to save gas
    // if amount <= u32::MAX, overflows do not happen
    if add {
        // quotient - `Δy / L` as U32.32
        let quotient = U128::from((amount as u128) << fixed_point_64::RESOLUTION) / liquidity;

        sqrt_p_x64.checked_add(quotient.as_u128()).unwrap()
    } else {
        let quotient = U128::div_rounding_up(
            U128::from((amount as u128) << fixed_point_64::RESOLUTION),
            U128::from(liquidity),
        );

        assert!(sqrt_p_x64 > quotient.as_u128());
        sqrt_p_x64 - quotient.as_u128()
    }
}

/// Gets the next sqrt price given an input amount of token_0 or token_1
/// Throws if price or liquidity are 0, or if the next price is out of bounds
///
/// # Arguments
///
/// * `sqrt_p_x64` - The starting price `√P`, i.e., before accounting for the input amount
/// * `liquidity` - The amount of usable liquidity
/// * `amount_in` - How much of token_0, or token_1, is being swapped in
/// * `zero_for_one` - Whether the amount in is token_0 or token_1
///
pub fn get_next_sqrt_price_from_input(
    sqrt_p_x64: u128,
    liquidity: u128,
    amount_in: u64,
    zero_for_one: bool,
) -> u128 {
    assert!(sqrt_p_x64 > 0);
    assert!(liquidity > 0);

    // round to make sure that we don't pass the target price
    if zero_for_one {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_p_x64, liquidity, amount_in, true)
    } else {
        get_next_sqrt_price_from_amount_1_rounding_down(sqrt_p_x64, liquidity, amount_in, true)
    }
}

/// Gets the next sqrt price given an output amount of token0 or token1
///
/// Throws if price or liquidity are 0 or the next price is out of bounds
///
/// # Arguments
///
/// * `sqrt_p_x64` - The starting price `√P`, i.e., before accounting for the output amount
/// * `liquidity` - The amount of usable liquidity
/// * `amount_out` - How much of token_0, or token_1, is being swapped out
/// * `zero_for_one` - Whether the amount out is token_0 or token_1
///
pub fn get_next_sqrt_price_from_output(
    sqrt_p_x64: u128,
    liquidity: u128,
    amount_out: u64,
    zero_for_one: bool,
) -> u128 {
    assert!(sqrt_p_x64 > 0);
    assert!(liquidity > 0);

    if zero_for_one {
        get_next_sqrt_price_from_amount_1_rounding_down(sqrt_p_x64, liquidity, amount_out, false)
    } else {
        get_next_sqrt_price_from_amount_0_rounding_up(sqrt_p_x64, liquidity, amount_out, false)
    }
}


