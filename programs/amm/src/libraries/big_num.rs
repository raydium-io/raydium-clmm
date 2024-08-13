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

#[macro_export]
macro_rules! construct_bignum {
    ( $(#[$attr:meta])* $visibility:vis struct $name:ident ( $n_words:tt ); ) => {
        $crate::construct_bignum! { @construct $(#[$attr])* $visibility struct $name ($n_words); }
        impl $crate::core_::convert::From<u128> for $name {
            fn from(value: u128) -> $name {
                let mut ret = [0; $n_words];
                ret[0] = value as u64;
                ret[1] = (value >> 64) as u64;
                $name(ret)
            }
        }

        impl $crate::core_::convert::From<i128> for $name {
            fn from(value: i128) -> $name {
                match value >= 0 {
                    true => From::from(value as u128),
                    false => { panic!("Unsigned integer can't be created from negative value"); }
                }
            }
        }

        impl $name {
            /// Low 2 words (u128)
            #[inline]
            pub const fn low_u128(&self) -> u128 {
                let &$name(ref arr) = self;
                ((arr[1] as u128) << 64) + arr[0] as u128
            }

            /// Conversion to u128 with overflow checking
            ///
            /// # Panics
            ///
            /// Panics if the number is larger than 2^128.
            #[inline]
            pub fn as_u128(&self) -> u128 {
                let &$name(ref arr) = self;
                for i in 2..$n_words {
                    if arr[i] != 0 {
                        panic!("Integer overflow when casting to u128")
                    }

                }
                self.low_u128()
            }
        }

        impl $crate::core_::convert::TryFrom<$name> for u128 {
            type Error = &'static str;

            #[inline]
            fn try_from(u: $name) -> $crate::core_::result::Result<u128, &'static str> {
                let $name(arr) = u;
                for i in 2..$n_words {
                    if arr[i] != 0 {
                        return Err("integer overflow when casting to u128");
                    }
                }
                Ok(((arr[1] as u128) << 64) + arr[0] as u128)
            }
        }

        impl $crate::core_::convert::TryFrom<$name> for i128 {
            type Error = &'static str;

            #[inline]
            fn try_from(u: $name) -> $crate::core_::result::Result<i128, &'static str> {
                let err_str = "integer overflow when casting to i128";
                let i = u128::try_from(u).map_err(|_| err_str)?;
                if i > i128::max_value() as u128 {
                    Err(err_str)
                } else {
                    Ok(i as i128)
                }
            }
        }
    };

    ( @construct $(#[$attr:meta])* $visibility:vis struct $name:ident ( $n_words:tt ); ) => {
		/// Little-endian large integer type
		#[repr(C)]
		$(#[$attr])*
		#[derive(Copy, Clone, Eq, PartialEq, Hash)]
		$visibility struct $name (pub [u64; $n_words]);

		/// Get a reference to the underlying little-endian words.
		impl AsRef<[u64]> for $name {
			#[inline]
			fn as_ref(&self) -> &[u64] {
				&self.0
			}
		}

		impl<'a> From<&'a $name> for $name {
			fn from(x: &'a $name) -> $name {
				*x
			}
		}

        impl $name {
			/// Maximum value.
			pub const MAX: $name = $name([u64::max_value(); $n_words]);

            /// Conversion to usize with overflow checking
			///
			/// # Panics
			///
			/// Panics if the number is larger than usize::max_value().
			#[inline]
			pub fn as_usize(&self) -> usize {
				let &$name(ref arr) = self;
				if !self.fits_word() || arr[0] > usize::max_value() as u64 {
					panic!("Integer overflow when casting to usize")
				}
				arr[0] as usize
			}

			/// Whether this is zero.
			#[inline]
			pub const fn is_zero(&self) -> bool {
				let &$name(ref arr) = self;
				let mut i = 0;
				while i < $n_words { if arr[i] != 0 { return false; } else { i += 1; } }
				return true;
			}

            // Whether this fits u64.
			#[inline]
			fn fits_word(&self) -> bool {
				let &$name(ref arr) = self;
				for i in 1..$n_words { if arr[i] != 0 { return false; } }
				return true;
			}

            /// Return if specific bit is set.
			///
			/// # Panics
			///
			/// Panics if `index` exceeds the bit width of the number.
			#[inline]
			pub const fn bit(&self, index: usize) -> bool {
				let &$name(ref arr) = self;
				arr[index / 64] & (1 << (index % 64)) != 0
			}

            /// Returns the number of leading zeros in the binary representation of self.
			pub fn leading_zeros(&self) -> u32 {
				let mut r = 0;
				for i in 0..$n_words {
					let w = self.0[$n_words - i - 1];
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
				for i in 0..$n_words {
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

            /// Zero (additive identity) of this type.
			#[inline]
			pub const fn zero() -> Self {
				Self([0; $n_words])
			}

			/// One (multiplicative identity) of this type.
			#[inline]
			pub const fn one() -> Self {
				let mut words = [0; $n_words];
				words[0] = 1u64;
				Self(words)
			}

			/// The maximum value which can be inhabited by this type.
			#[inline]
			pub const fn max_value() -> Self {
				Self::MAX
			}
        }

        impl $crate::core_::default::Default for $name {
            fn default() -> Self {
                $name::zero()
            }
        }

        impl $crate::core_::ops::BitAnd<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitand(self, other: $name) -> $name {
                let $name(ref arr1) = self;
                let $name(ref arr2) = other;
                let mut ret = [0u64; $n_words];
                for i in 0..$n_words {
                    ret[i] = arr1[i] & arr2[i];
                }
                $name(ret)
            }
        }

        impl $crate::core_::ops::BitOr<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitor(self, other: $name) -> $name {
                let $name(ref arr1) = self;
                let $name(ref arr2) = other;
                let mut ret = [0u64; $n_words];
                for i in 0..$n_words {
                    ret[i] = arr1[i] | arr2[i];
                }
                $name(ret)
            }
        }

        impl $crate::core_::ops::BitXor<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitxor(self, other: $name) -> $name {
                let $name(ref arr1) = self;
                let $name(ref arr2) = other;
                let mut ret = [0u64; $n_words];
                for i in 0..$n_words {
                    ret[i] = arr1[i] ^ arr2[i];
                }
                $name(ret)
            }
        }

        impl $crate::core_::ops::Not for $name {
            type Output = $name;

            #[inline]
            fn not(self) -> $name {
                let $name(ref arr) = self;
                let mut ret = [0u64; $n_words];
                for i in 0..$n_words {
                    ret[i] = !arr[i];
                }
                $name(ret)
            }
        }

        impl $crate::core_::ops::Shl<usize> for $name {
            type Output = $name;

            fn shl(self, shift: usize) -> $name {
                let $name(ref original) = self;
                let mut ret = [0u64; $n_words];
                let word_shift = shift / 64;
                let bit_shift = shift % 64;

                // shift
                for i in word_shift..$n_words {
                    ret[i] = original[i - word_shift] << bit_shift;
                }
                // carry
                if bit_shift > 0 {
                    for i in word_shift+1..$n_words {
                        ret[i] += original[i - 1 - word_shift] >> (64 - bit_shift);
                    }
                }
                $name(ret)
            }
        }

        impl<'a> $crate::core_::ops::Shl<usize> for &'a $name {
            type Output = $name;
            fn shl(self, shift: usize) -> $name {
                *self << shift
            }
        }

        impl $crate::core_::ops::Shr<usize> for $name {
            type Output = $name;

            fn shr(self, shift: usize) -> $name {
                let $name(ref original) = self;
                let mut ret = [0u64; $n_words];
                let word_shift = shift / 64;
                let bit_shift = shift % 64;

                // shift
                for i in word_shift..$n_words {
                    ret[i - word_shift] = original[i] >> bit_shift;
                }

                // Carry
                if bit_shift > 0 {
                    for i in word_shift+1..$n_words {
                        ret[i - word_shift - 1] += original[i] << (64 - bit_shift);
                    }
                }

                $name(ret)
            }
        }

        impl<'a> $crate::core_::ops::Shr<usize> for &'a $name {
            type Output = $name;
            fn shr(self, shift: usize) -> $name {
                *self >> shift
            }
        }
    };
}
construct_bignum! {
    pub struct U1024(16);
}
