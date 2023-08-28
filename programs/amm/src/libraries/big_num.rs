///! 128 and 256 bit numbers
///! U128 is more efficient that u128
///! https://github.com/solana-labs/solana/issues/19549
use uint::construct_uint;

construct_uint! {
    pub struct U128(2);
}

construct_uint! {
    pub struct U256(4);
}

construct_uint! {
    pub struct U512(8);
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, PartialOrd, Eq, Ord)]
pub struct U1024(pub [u64; 16]);
const N_WORDS: usize = 16;
impl U1024 {
    /// Whether this is zero.
    #[inline]
    pub const fn is_zero(&self) -> bool {
        let mut i = 0;
        while i < 16 {
            if self.0[i] != 0 {
                return false;
            } else {
                i += 1;
            }
        }
        return true;
    }

    /// Zero (additive identity) of this type.
    #[inline]
    pub const fn zero() -> Self {
        Self([0; N_WORDS])
    }

    /// One (multiplicative identity) of this type.
    #[inline]
    pub const fn one() -> Self {
        let mut words = [0; N_WORDS];
        words[0] = 1u64;
        Self(words)
    }

    /// The maximum value which can be inhabited by this type.
    #[inline]
    pub fn max_value() -> Self {
        let mut result = [0; N_WORDS];
        for i in 0..N_WORDS {
            result[i] = u64::max_value();
        }
        U1024(result)
    }

    /// Conversion to usize with overflow checking
    ///
    /// # Panics
    ///
    /// Panics if the number is larger than usize::max_value().
    #[inline]
    pub fn as_usize(&self) -> usize {
        let arr = self.0;
        if !self.fits_word() || arr[0] > u64::try_from(usize::max_value()).unwrap() {
            panic!("Integer overflow when casting to usize")
        }
        arr[0] as usize
    }

    // Whether this fits u64.
    #[inline]
    fn fits_word(&self) -> bool {
        let arr = self.0;
        for i in 1..N_WORDS {
            if arr[i] != 0 {
                return false;
            }
        }
        return true;
    }

    /// Returns the number of leading zeros in the binary representation of self.
    pub fn leading_zeros(&self) -> u32 {
        let mut r = 0;
        for i in 0..N_WORDS {
            let w = self.0[N_WORDS - i - 1];
            if w == 0 {
                r += 64;
            } else {
                r += w.leading_zeros();
                break;
            }
        }
        r
    }

    /// Returns the number of trailing zeros in the binary representation of self.
    pub fn trailing_zeros(&self) -> u32 {
        let mut r = 0;
        for i in 0..N_WORDS {
            let w = self.0[i];
            if w == 0 {
                r += 64;
            } else {
                r += w.trailing_zeros();
                break;
            }
        }
        r
    }

    /// Return if specific bit is set.
    ///
    /// # Panics
    ///
    /// Panics if `index` exceeds the bit width of the number.
    #[inline]
    pub const fn bit(&self, index: usize) -> bool {
        self.0[index / 64] & (1 << (index % 64)) != 0
    }
}

impl core::default::Default for U1024 {
    fn default() -> Self {
        U1024::zero()
    }
}

impl core::ops::BitAnd<U1024> for U1024 {
    type Output = U1024;
    fn bitand(self, other: U1024) -> U1024 {
        let arr1 = self.0;
        let arr2 = other.0;
        let mut ret = [0u64; N_WORDS];
        for i in 0..N_WORDS {
            ret[i] = arr1[i] & arr2[i];
        }
        U1024(ret)
    }
}

impl core::ops::BitXor<U1024> for U1024 {
    type Output = U1024;
    fn bitxor(self, other: U1024) -> U1024 {
        let arr1 = self.0;
        let arr2 = other.0;
        let mut ret = [0u64; N_WORDS];
        for i in 0..N_WORDS {
            ret[i] = arr1[i] ^ arr2[i];
        }
        U1024(ret)
    }
}

impl core::ops::BitOr<U1024> for U1024 {
    type Output = U1024;
    fn bitor(self, other: U1024) -> U1024 {
        let arr1 = self.0;
        let arr2 = other.0;
        let mut ret = [0u64; N_WORDS];
        for i in 0..N_WORDS {
            ret[i] = arr1[i] | arr2[i];
        }
        U1024(ret)
    }
}

impl core::ops::Not for U1024 {
    type Output = U1024;
    fn not(self) -> U1024 {
        let arr = self.0;
        let mut ret = [0u64; N_WORDS];
        for i in 0..N_WORDS {
            ret[i] = !arr[i];
        }
        U1024(ret)
    }
}
impl core::ops::Shl<usize> for U1024 {
    type Output = U1024;

    fn shl(self, shift: usize) -> U1024 {
        let original = self.0;
        let mut ret = [0u64; N_WORDS];
        let word_shift = shift / 64;
        let bit_shift = shift % 64;

        // shift
        for i in word_shift..N_WORDS {
            ret[i] = original[i - word_shift] << bit_shift;
        }
        // carry
        if bit_shift > 0 {
            for i in word_shift + 1..N_WORDS {
                ret[i] += original[i - 1 - word_shift] >> (64 - bit_shift);
            }
        }
        U1024(ret)
    }
}
impl<'a> core::ops::Shl<usize> for &'a U1024 {
    type Output = U1024;
    fn shl(self, shift: usize) -> U1024 {
        *self << shift
    }
}

impl core::ops::Shr<usize> for U1024 {
    type Output = U1024;

    fn shr(self, shift: usize) -> U1024 {
        let original = self.0;
        let mut ret = [0u64; N_WORDS];
        let word_shift = shift / 64;
        let bit_shift = shift % 64;

        // shift
        for i in word_shift..N_WORDS {
            ret[i - word_shift] = original[i] >> bit_shift;
        }

        // Carry
        if bit_shift > 0 {
            for i in word_shift + 1..N_WORDS {
                ret[i - word_shift - 1] += original[i] << (64 - bit_shift);
            }
        }

        U1024(ret)
    }
}
impl<'a> core::ops::Shr<usize> for &'a U1024 {
    type Output = U1024;
    fn shr(self, shift: usize) -> U1024 {
        *self >> shift
    }
}
