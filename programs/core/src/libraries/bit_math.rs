///! Helper functions to get most and least significant non-zero bits
use super::big_num::U256;

/// Returns index of the most significant non-zero bit of the number
///
/// The function satisfies the property:
///     x >= 2**most_significant_bit(x) and x < 2**(most_significant_bit(x)+1)
///
/// # Arguments
///
/// * `x` - the value for which to compute the most significant bit, must be greater than 0
///
pub fn most_significant_bit(x: U256) -> u8 {
    assert!(x > U256::default());
    255 - x.leading_zeros() as u8
}

/// Returns index of the least significant non-zero bit of the number
///
/// The function satisfies the property:
///     (x & 2**leastSignificantBit(x)) != 0 and (x & (2**(leastSignificantBit(x)) - 1)) == 0)
///
///
/// # Arguments
///
/// * `x` - the value for which to compute the least significant bit, must be greater than 0
///
pub fn least_significant_bit(x: U256) -> u8 {
    assert!(x > U256::default());
    x.trailing_zeros() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    mod most_significant_bit {
        use super::*;

        #[test]
        fn test_msb_at_powers_of_two() {
            for i in 0..255 {
                let a = U256::from(1) << i;
                assert_eq!(most_significant_bit(a), i);
            }
        }

        #[test]
        #[should_panic]
        fn test_msb_for_0() {
            most_significant_bit(U256::default());
        }

        #[test]
        fn test_msb_for_1() {
            assert_eq!(most_significant_bit(U256::from(1)), 0);
        }

        #[test]
        fn test_msb_for_2() {
            assert_eq!(most_significant_bit(U256::from(2)), 1);
        }

        #[test]
        fn test_msb_for_max() {
            assert_eq!(most_significant_bit(U256::MAX), 255);
        }
    }

    mod least_significant_bit {
        use super::*;

        #[test]
        fn test_lsb_at_powers_of_two() {
            for i in 0..255 {
                let a = U256::from(1) << i;
                assert_eq!(least_significant_bit(a), i);
            }
        }

        #[test]
        #[should_panic]
        fn test_lsb_for_0() {
            least_significant_bit(U256::default());
        }

        #[test]
        fn test_lsb_for_1() {
            assert_eq!(least_significant_bit(U256::from(1)), 0);
        }

        #[test]
        fn test_lsb_for_2() {
            assert_eq!(least_significant_bit(U256::from(2)), 1);
        }

        #[test]
        fn test_lsb_for_max() {
            assert_eq!(least_significant_bit(U256::MAX), 0);
        }
    }
}
