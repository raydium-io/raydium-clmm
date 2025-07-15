use crate::{error::ErrorCode, libraries::big_num::U128};

use anchor_lang::require;

/// The minimum tick
pub const MIN_TICK: i32 = -443636;
/// The minimum tick
pub const MAX_TICK: i32 = -MIN_TICK;

/// The minimum value that can be returned from #get_sqrt_price_at_tick. Equivalent to get_sqrt_price_at_tick(MIN_TICK)
pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
/// The maximum value that can be returned from #get_sqrt_price_at_tick. Equivalent to get_sqrt_price_at_tick(MAX_TICK)
pub const MAX_SQRT_PRICE_X64: u128 = 79226673521066979257578248091;

// Number 64, encoded as a U128
const NUM_64: U128 = U128([64, 0]);

const BIT_PRECISION: u32 = 16;

/// Calculates 1.0001^(tick/2) as a U64.64 number representing
/// the square root of the ratio of the two assets (token_1/token_0)
///
/// Calculates result as a U64.64
/// Each magic factor is `2^64 / (1.0001^(2^(i - 1)))` for i in `[0, 18)`.
///
/// Throws if |tick| > MAX_TICK
///
/// # Arguments
/// * `tick` - Price tick
///
pub fn get_sqrt_price_at_tick(tick: i32) -> Result<u128, anchor_lang::error::Error> {
    let abs_tick = tick.abs() as u32;
    require!(abs_tick <= MAX_TICK as u32, ErrorCode::TickUpperOverflow);

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
    // i = 18
    if abs_tick & 0x40000 != 0 {
        ratio = (ratio * U128([0x2216e584f5fa, 0])) >> NUM_64
    }

    // Divide to obtain 1.0001^(2^(i - 1)) * 2^32 in numerator
    if tick > 0 {
        ratio = U128::MAX / ratio;
    }

    Ok(ratio.as_u128())
}

/// Calculates the greatest tick value such that get_sqrt_price_at_tick(tick) <= ratio
/// Throws if sqrt_price_x64 < MIN_SQRT_RATIO or sqrt_price_x64 > MAX_SQRT_RATIO
///
/// Formula: `i = log base(√1.0001) (√P)`
pub fn get_tick_at_sqrt_price(sqrt_price_x64: u128) -> Result<i32, anchor_lang::error::Error> {
    // second inequality must be < because the price can never reach the price at the max tick
    require!(
        sqrt_price_x64 >= MIN_SQRT_PRICE_X64 && sqrt_price_x64 < MAX_SQRT_PRICE_X64,
        ErrorCode::SqrtPriceX64
    );

    // Determine log_b(sqrt_ratio). First by calculating integer portion (msb)
    let msb: u32 = 128 - sqrt_price_x64.leading_zeros() - 1;
    let log2p_integer_x32 = (msb as i128 - 64) << 32;

    // get fractional value (r/2^msb), msb always > 128
    // We begin the iteration from bit 63 (0.5 in Q64.64)
    let mut bit: i128 = 0x8000_0000_0000_0000i128;
    let mut precision = 0;
    let mut log2p_fraction_x64 = 0;

    // Log2 iterative approximation for the fractional part
    // Go through each 2^(j) bit where j < 64 in a Q64.64 number
    // Append current bit value to fraction result if r^2 Q2.126 is more than 2
    let mut r = if msb >= 64 {
        sqrt_price_x64 >> (msb - 63)
    } else {
        sqrt_price_x64 << (63 - msb)
    };

    while bit > 0 && precision < BIT_PRECISION {
        r *= r;
        let is_r_more_than_two = r >> 127 as u32;
        r >>= 63 + is_r_more_than_two;
        log2p_fraction_x64 += bit * is_r_more_than_two as i128;
        bit >>= 1;
        precision += 1;
    }
    let log2p_fraction_x32 = log2p_fraction_x64 >> 32;
    let log2p_x32 = log2p_integer_x32 + log2p_fraction_x32;

    // 14 bit refinement gives an error margin of 2^-14 / log2 (√1.0001) = 0.8461 < 1
    // Since tick is a decimal, an error under 1 is acceptable

    // Change of base rule: multiply with 2^32 / log2 (√1.0001)
    let log_sqrt_10001_x64 = log2p_x32 * 59543866431248i128;

    // tick - 0.01
    let tick_low = ((log_sqrt_10001_x64 - 184467440737095516i128) >> 64) as i32;

    // tick + (2^-14 / log2(√1.0001)) + 0.01
    let tick_high = ((log_sqrt_10001_x64 + 15793534762490258745i128) >> 64) as i32;

    Ok(if tick_low == tick_high {
        tick_low
    } else if get_sqrt_price_at_tick(tick_high).unwrap() <= sqrt_price_x64 {
        tick_high
    } else {
        tick_low
    })
}

#[cfg(test)]
mod tick_math_test {
    use super::*;
    mod get_sqrt_price_at_tick_test {
        use super::*;
        use crate::libraries::fixed_point_64;

        #[test]
        fn check_get_sqrt_price_at_tick_at_min_or_max_tick() {
            assert_eq!(
                get_sqrt_price_at_tick(MIN_TICK).unwrap(),
                MIN_SQRT_PRICE_X64
            );
            let min_sqrt_price = MIN_SQRT_PRICE_X64 as f64 / fixed_point_64::Q64 as f64;
            println!("min_sqrt_price: {}", min_sqrt_price);
            assert_eq!(
                get_sqrt_price_at_tick(MAX_TICK).unwrap(),
                MAX_SQRT_PRICE_X64
            );
            let max_sqrt_price = MAX_SQRT_PRICE_X64 as f64 / fixed_point_64::Q64 as f64;
            println!("max_sqrt_price: {}", max_sqrt_price);
        }
    }

    mod get_tick_at_sqrt_price_test {
        use super::*;

        #[test]
        fn check_get_tick_at_sqrt_price_at_min_or_max_sqrt_price() {
            assert_eq!(
                get_tick_at_sqrt_price(MIN_SQRT_PRICE_X64).unwrap(),
                MIN_TICK,
            );

            // we can't reach MAX_SQRT_PRICE_X64
            assert_eq!(
                get_tick_at_sqrt_price(MAX_SQRT_PRICE_X64 - 1).unwrap(),
                MAX_TICK - 1,
            );
        }
    }

    #[test]
    fn tick_round_down() {
        // tick is negative
        let sqrt_price_x64 = get_sqrt_price_at_tick(-28861).unwrap();
        let mut tick = get_tick_at_sqrt_price(sqrt_price_x64).unwrap();
        assert_eq!(tick, -28861);
        tick = get_tick_at_sqrt_price(sqrt_price_x64 + 1).unwrap();
        assert_eq!(tick, -28861);
        tick = get_tick_at_sqrt_price(get_sqrt_price_at_tick(-28860).unwrap() - 1).unwrap();
        assert_eq!(tick, -28861);
        tick = get_tick_at_sqrt_price(sqrt_price_x64 - 1).unwrap();
        assert_eq!(tick, -28862);

        // tick is positive
        let sqrt_price_x64 = get_sqrt_price_at_tick(28861).unwrap();
        tick = get_tick_at_sqrt_price(sqrt_price_x64).unwrap();
        assert_eq!(tick, 28861);
        tick = get_tick_at_sqrt_price(sqrt_price_x64 + 1).unwrap();
        assert_eq!(tick, 28861);
        tick = get_tick_at_sqrt_price(get_sqrt_price_at_tick(28862).unwrap() - 1).unwrap();
        assert_eq!(tick, 28861);
        tick = get_tick_at_sqrt_price(sqrt_price_x64 - 1).unwrap();
        assert_eq!(tick, 28860);
    }

    mod fuzz_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn get_sqrt_price_at_tick_test (
                tick in MIN_TICK+1..MAX_TICK-1,
            ) {
                let sqrt_price_x64 = get_sqrt_price_at_tick(tick).unwrap();

                assert!(sqrt_price_x64 >= MIN_SQRT_PRICE_X64);
                assert!(sqrt_price_x64 <= MAX_SQRT_PRICE_X64);

                let minus_tick_price_x64 = get_sqrt_price_at_tick(tick - 1).unwrap();
                let plus_tick_price_x64 = get_sqrt_price_at_tick(tick + 1).unwrap();
                assert!(minus_tick_price_x64 < sqrt_price_x64 && sqrt_price_x64 < plus_tick_price_x64);
            }

            #[test]
            fn get_tick_at_sqrt_price_test (
                sqrt_price in MIN_SQRT_PRICE_X64..MAX_SQRT_PRICE_X64
            ) {
                let tick = get_tick_at_sqrt_price(sqrt_price).unwrap();

                assert!(tick >= MIN_TICK);
                assert!(tick <= MAX_TICK);

                assert!(sqrt_price >= get_sqrt_price_at_tick(tick).unwrap() && sqrt_price < get_sqrt_price_at_tick(tick + 1).unwrap())
            }

            #[test]
            fn tick_and_sqrt_price_symmetry_test (
                tick in MIN_TICK..MAX_TICK
            ) {

                let sqrt_price_x64 = get_sqrt_price_at_tick(tick).unwrap();
                let resolved_tick = get_tick_at_sqrt_price(sqrt_price_x64).unwrap();
                assert!(resolved_tick == tick);
            }


            #[test]
            fn get_sqrt_price_at_tick_is_sequence_test (
                tick in MIN_TICK+1..MAX_TICK
            ) {

                let sqrt_price_x64 = get_sqrt_price_at_tick(tick).unwrap();
                let last_sqrt_price_x64 = get_sqrt_price_at_tick(tick-1).unwrap();
                assert!(last_sqrt_price_x64 < sqrt_price_x64);
            }

            #[test]
            fn get_tick_at_sqrt_price_is_sequence_test (
                sqrt_price in (MIN_SQRT_PRICE_X64 + 10)..MAX_SQRT_PRICE_X64
            ) {

                let tick = get_tick_at_sqrt_price(sqrt_price).unwrap();
                let last_tick = get_tick_at_sqrt_price(sqrt_price - 10).unwrap();
                assert!(last_tick <= tick);
            }
        }
    }
}
