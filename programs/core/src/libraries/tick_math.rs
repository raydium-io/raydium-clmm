use crate::{error::ErrorCode, libraries::big_num::U128};
///! Helper functions to calculate tick from √P and vice versa
///! Performs power and log calculations in a gas efficient manner
///!
///! Computes sqrt price for ticks of size 1.0001, i.e. sqrt(1.0001^tick) as fixed point Q32.32 numbers. Supports
///! prices between 2**-32 and 2**32
///!
///! # Resources
///!
///! * https://medium.com/coinmonks/math-in-solidity-part-5-exponent-and-logarithm-9aef8515136e
///! * https://liaoph.com/logarithm-in-solidity/
///!
use anchor_lang::require;

/// The minimum tick that may be passed to #get_sqrt_ratio_at_tick computed from log base 1.0001 of 2**-32
pub const MIN_TICK: i32 = -221818;
/// The minimum tick that may be passed to #get_sqrt_ratio_at_tick computed from log base 1.0001 of 2**32
pub const MAX_TICK: i32 = -MIN_TICK;

/// The minimum value that can be returned from #get_sqrt_ratio_at_tick. Equivalent to get_sqrt_ratio_at_tick(MIN_TICK)
pub const MIN_SQRT_RATIO: u64 = 65537;
/// The maximum value that can be returned from #get_sqrt_ratio_at_tick. Equivalent to get_sqrt_ratio_at_tick(MAX_TICK)
pub const MAX_SQRT_RATIO: u64 = 281472331703918;

// Number 64, encoded as a U128
const NUM_64: U128 = U128([64, 0]);

/// Calculates 1.0001^(tick/2) as a U32.32 number representing
/// the square root of the ratio of the two assets (token_1/token_0)
///
/// Calculates result as a U64.64, then rounds down to U32.32.
/// Each magic factor is `2^64 / (1.0001^(2^(i - 1)))` for i in `[0, 18)`.
///
/// Uniswap follows `2^128 / (1.0001^(2^(i - 1)))` for i in [0, 20), for U128.128
///
/// Throws if |tick| > MAX_TICK
///
/// # Arguments
/// * `tick` - Price tick
///
pub fn get_sqrt_ratio_at_tick(tick: i32) -> Result<u64, anchor_lang::error::Error> {
    let abs_tick = tick.abs() as u32;
    require!(abs_tick <= MAX_TICK as u32, ErrorCode::T);

    // i = 0
    let mut ratio = if abs_tick & 0x1 != 0 {
        U128([0xfffcb933bd6fb800, 0])
    } else {
        // 2^64
        U128([0, 1])
    };
    // i = 1
    if abs_tick & 0x2 != 0 {
        ratio = (ratio * U128([0xfff97272373d4000, 0])) >> NUM_64
    };
    // i = 2
    if abs_tick & 0x4 != 0 {
        ratio = (ratio * U128([0xfff2e50f5f657000, 0])) >> NUM_64
    };
    // i = 3
    if abs_tick & 0x8 != 0 {
        ratio = (ratio * U128([0xffe5caca7e10f000, 0])) >> NUM_64
    };
    // i = 4
    if abs_tick & 0x10 != 0 {
        ratio = (ratio * U128([0xffcb9843d60f7000, 0])) >> NUM_64
    };
    // i = 5
    if abs_tick & 0x20 != 0 {
        ratio = (ratio * U128([0xff973b41fa98e800, 0])) >> NUM_64
    };
    // i = 6
    if abs_tick & 0x40 != 0 {
        ratio = (ratio * U128([0xff2ea16466c9b000, 0])) >> NUM_64
    };
    // i = 7
    if abs_tick & 0x80 != 0 {
        ratio = (ratio * U128([0xfe5dee046a9a3800, 0])) >> NUM_64
    };
    // i = 8
    if abs_tick & 0x100 != 0 {
        ratio = (ratio * U128([0xfcbe86c7900bb000, 0])) >> NUM_64
    };
    // i = 9
    if abs_tick & 0x200 != 0 {
        ratio = (ratio * U128([0xf987a7253ac65800, 0])) >> NUM_64
    };
    // i = 10
    if abs_tick & 0x400 != 0 {
        ratio = (ratio * U128([0xf3392b0822bb6000, 0])) >> NUM_64
    };
    // i = 11
    if abs_tick & 0x800 != 0 {
        ratio = (ratio * U128([0xe7159475a2caf000, 0])) >> NUM_64
    };
    // i = 12
    if abs_tick & 0x1000 != 0 {
        ratio = (ratio * U128([0xd097f3bdfd2f2000, 0])) >> NUM_64
    };
    // i = 13
    if abs_tick & 0x2000 != 0 {
        ratio = (ratio * U128([0xa9f746462d9f8000, 0])) >> NUM_64
    };
    // i = 14
    if abs_tick & 0x4000 != 0 {
        ratio = (ratio * U128([0x70d869a156f31c00, 0])) >> NUM_64
    };
    // i = 15
    if abs_tick & 0x8000 != 0 {
        ratio = (ratio * U128([0x31be135f97ed3200, 0])) >> NUM_64
    };
    // i = 16
    if abs_tick & 0x10000 != 0 {
        ratio = (ratio * U128([0x9aa508b5b85a500, 0])) >> NUM_64
    };
    // i = 17
    if abs_tick & 0x20000 != 0 {
        ratio = (ratio * U128([0x5d6af8dedc582c, 0])) >> NUM_64
    };

    // Divide to obtain 1.0001^(2^(i - 1)) * 2^32 in numerator
    if tick > 0 {
        ratio = U128::MAX / ratio;
    }

    // Rounding up and convert to U32.32
    let sqrt_price_x32 = (ratio >> U128([32, 0])).as_u64()
        + ((ratio % U128([1_u64 << 32, 0]) != U128::default()) as u64);

    Ok(sqrt_price_x32)
}

/// Calculates the greatest tick value such that get_sqrt_ratio_at_tick(tick) <= ratio
/// Throws if sqrt_price_x32 < MIN_SQRT_RATIO or sqrt_price_x32 > MAX_SQRT_RATIO
///
/// Formula: `i = log base(√1.0001) (√P)`
///
/// # Arguments
///
/// * `sqrt_price_x32`- The sqrt ratio for which to compute the tick as a U32.32
///
pub fn get_tick_at_sqrt_ratio(sqrt_price_x32: u64) -> Result<i32, anchor_lang::error::Error> {
    // second inequality must be < because the price can never reach the price at the max tick
    require!(
        sqrt_price_x32 >= MIN_SQRT_RATIO && sqrt_price_x32 < MAX_SQRT_RATIO,
        ErrorCode::R
    );

    let mut r = sqrt_price_x32;
    let mut msb = 0; // in [1, 64)

    // ------------------------------------------------------
    // Decimal part of logarithm = MSB
    // Binary search method: 2^32, 2^16, 2^8, 2^4, 2^2 and 2^1 for U32.32

    let mut f: u8 = ((r >= 0x100000000) as u8) << 5; // If r >= 2^32, f = 32 else 0
    msb |= f; // Add f to MSB
    r >>= f; // Right shift by f

    f = ((r >= 0x10000) as u8) << 4; // 2^16
    msb |= f;
    r >>= f;

    f = ((r >= 0x100) as u8) << 3; // 2^8
    msb |= f;
    r >>= f;

    f = ((r >= 0x10) as u8) << 2; // 2^4
    msb |= f;
    r >>= f;

    f = ((r >= 0x4) as u8) << 1; // 2^2
    msb |= f;
    r >>= f;

    f = ((r >= 0x2) as u8) << 0; // 2^0
    msb |= f;

    // log2 (m x 2^e) = log2 (m) + e
    // For U32.32, e = -32. Subtract by 32 to remove x32 notation.
    // Then left shift by 16 bits to convert into U48.16 form
    let mut log_2_x16 = (msb as i64 - 32) << 16;

    // ------------------------------------------------------
    // Fractional part of logarithm

    // Set r = r / 2^n as a Q33.31 number, where n stands for msb
    r = if msb >= 32 {
        sqrt_price_x32 >> (msb - 31)
    } else {
        sqrt_price_x32 << (31 - msb)
    };

    r = (r * r) >> 31; // r^2 as U33.31
    f = (r >> 32) as u8; // MSB of r^2 (0 or 1)
    log_2_x16 |= (f as i64) << 15; // Add f at 1st fractional place
    r >>= f; // Divide r by 2 if MSB of f is non-zero

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 14;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 13;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 12;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 11;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 10;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 9;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 8;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 7;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 6;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 5;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 4;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 3;
    r >>= f;

    r = (r * r) >> 31;
    f = (r >> 32) as u8;
    log_2_x16 |= (f as i64) << 2;

    // 14 bit refinement gives an error margin of 2^-14 / log2 (√1.0001) = 0.8461 < 1
    // Since tick is a decimal, an error under 1 is acceptable

    // Change of base rule: multiply with 2^16 / log2 (√1.0001)
    let log_sqrt_10001_x32 = log_2_x16 * 908567298;

    // tick - 0.01
    let tick_low = ((log_sqrt_10001_x32 - 42949672) >> 32) as i32;

    // tick + (2^-14 / log2(√1.001)) + 0.01
    let tick_high = ((log_sqrt_10001_x32 + 3677218864) >> 32) as i32;

    Ok(if tick_low == tick_high {
        tick_low
    } else if get_sqrt_ratio_at_tick(tick_high).unwrap() <= sqrt_price_x32 {
        tick_high
    } else {
        tick_low
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod get_sqrt_ratio_at_tick {
        use crate::libraries::test_utils::encode_price_sqrt_x32;

        use super::*;

        #[test]
        #[should_panic]
        fn throws_for_too_low() {
            get_sqrt_ratio_at_tick(MIN_TICK - 1).unwrap();
        }

        #[test]
        #[should_panic]
        fn throws_for_too_high() {
            get_sqrt_ratio_at_tick(MAX_TICK + 1).unwrap();
        }

        #[test]
        fn min_tick() {
            assert_eq!(get_sqrt_ratio_at_tick(MIN_TICK).unwrap(), MIN_SQRT_RATIO);
        }

        #[test]
        fn min_tick_plus_one() {
            assert_eq!(get_sqrt_ratio_at_tick(MIN_TICK + 1).unwrap(), 65540);
        }

        #[test]
        fn max_tick() {
            assert_eq!(get_sqrt_ratio_at_tick(MAX_TICK).unwrap(), MAX_SQRT_RATIO);
        }

        #[test]
        fn max_tick_minus_one() {
            assert_eq!(
                get_sqrt_ratio_at_tick(MAX_TICK - 1).unwrap(),
                281458259142766
            );
        }

        #[test]
        fn min_tick_ratio_is_less_than_js_implementation() {
            assert!(
                get_sqrt_ratio_at_tick(MIN_TICK).unwrap()
                    < encode_price_sqrt_x32(1, u64::pow(2, 31))
            );
        }

        #[test]
        fn max_tick_ratio_is_greater_than_js_implementation() {
            assert!(
                get_sqrt_ratio_at_tick(MAX_TICK).unwrap()
                    > encode_price_sqrt_x32(u64::pow(2, 31), 1)
            );
        }

        #[test]
        fn is_at_most_off_by_a_bip() {
            // 1/100th of a bip condition holds for positive ticks
            let _abs_ticks: Vec<i32> = vec![
                50, 100, 250, 500, 1_000, 2_500, 3_000, 4_000, 5_000, 50_000, 150_000,
            ];

            for tick in MIN_TICK..=MAX_TICK {
                let result = get_sqrt_ratio_at_tick(tick).unwrap();
                let float_result = (f64::powi(1.0001, tick).sqrt() * u64::pow(2, 32) as f64) as u64;
                let abs_diff = if result > float_result {
                    result - float_result
                } else {
                    float_result - result
                };
                assert!((abs_diff as f64 / result as f64) < 0.0001);
            }
        }

        #[test]
        fn original_tick_can_be_retrieved_from_sqrt_ratio() {
            for tick in MIN_TICK..=MAX_TICK {
                let sqrt_price_x32 = get_sqrt_ratio_at_tick(tick).unwrap();
                if sqrt_price_x32 < MAX_SQRT_RATIO {
                    let obtained_tick = get_tick_at_sqrt_ratio(sqrt_price_x32).unwrap();
                    assert_eq!(tick, obtained_tick);
                }
            }
        }

        #[test]
        fn sqrt_price_increases_with_tick() {
            let mut prev_price_x32: u64 = 0;
            for tick in MIN_TICK..=MAX_TICK {
                let sqrt_price_x32 = get_sqrt_ratio_at_tick(tick).unwrap();
                // P should increase with tick
                if prev_price_x32 != 0 {
                    assert!(sqrt_price_x32 > prev_price_x32);
                }
                prev_price_x32 = sqrt_price_x32;
            }
        }
    }

    mod get_tick_at_sqrt_ratio {
        use crate::libraries::test_utils::encode_price_sqrt_x32;

        use super::*;

        #[test]
        #[should_panic(expected = "R")]
        fn throws_for_too_low() {
            get_tick_at_sqrt_ratio(MIN_SQRT_RATIO - 1).unwrap();
        }

        #[test]
        #[should_panic(expected = "R")]
        fn throws_for_too_high() {
            get_tick_at_sqrt_ratio(MAX_SQRT_RATIO).unwrap();
        }

        #[test]
        fn ratio_of_min_tick() {
            assert_eq!(get_tick_at_sqrt_ratio(MIN_SQRT_RATIO).unwrap(), MIN_TICK);
        }

        #[test]
        fn ratio_of_min_tick_plus_one() {
            assert_eq!(get_tick_at_sqrt_ratio(65540).unwrap(), MIN_TICK + 1);
        }

        #[test]
        fn ratio_of_max_tick_minus_one() {
            assert_eq!(
                get_tick_at_sqrt_ratio(281458259142766).unwrap(),
                MAX_TICK - 1
            );
        }

        #[test]
        fn ratio_closest_to_max_tick() {
            assert_eq!(
                get_tick_at_sqrt_ratio(MAX_SQRT_RATIO - 1).unwrap(),
                MAX_TICK - 1
            );
        }

        #[test]
        fn is_off_by_at_most_one() {
            let sqrt_ratios: Vec<u64> = vec![
                encode_price_sqrt_x32(u64::pow(10, 6), 1),
                encode_price_sqrt_x32(1, 64),
                encode_price_sqrt_x32(1, 8),
                encode_price_sqrt_x32(1, 2),
                encode_price_sqrt_x32(1, 1),
                encode_price_sqrt_x32(2, 1),
                encode_price_sqrt_x32(8, 1),
                encode_price_sqrt_x32(64, 1),
                encode_price_sqrt_x32(1, u64::pow(10, 6)),
            ];
            for sqrt_ratio_x32 in sqrt_ratios.iter() {
                let float_result = f64::log(
                    *sqrt_ratio_x32 as f64 / u64::pow(2, 32) as f64,
                    f64::sqrt(1.0001),
                );
                let result = get_tick_at_sqrt_ratio(*sqrt_ratio_x32).unwrap() as f64;

                let abs_diff = if result > float_result {
                    result - float_result
                } else {
                    float_result - result
                };
                assert!(abs_diff < 1.0);
            }
        }

        #[test]
        fn ratio_is_between_tick_and_tick_plus_one() {
            let sqrt_ratios: Vec<u64> = vec![
                encode_price_sqrt_x32(u64::pow(10, 6), 1),
                encode_price_sqrt_x32(1, 64),
                encode_price_sqrt_x32(1, 8),
                encode_price_sqrt_x32(1, 2),
                encode_price_sqrt_x32(1, 1),
                encode_price_sqrt_x32(2, 1),
                encode_price_sqrt_x32(8, 1),
                encode_price_sqrt_x32(64, 1),
                encode_price_sqrt_x32(1, u64::pow(10, 6)),
            ];
            for ratio in sqrt_ratios.iter() {
                let tick = get_tick_at_sqrt_ratio(*ratio).unwrap();
                let ratio_of_tick = get_sqrt_ratio_at_tick(tick).unwrap();
                let ratio_of_tick_plus_one = get_sqrt_ratio_at_tick(tick + 1).unwrap();

                assert!(*ratio >= ratio_of_tick);
                assert!(*ratio < ratio_of_tick_plus_one);
            }
        }
    }
}
