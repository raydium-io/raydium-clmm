//! A custom implementation of https://github.com/sdroege/rust-muldiv to support phantom overflow resistant
//! multiply-divide operations. This library uses U128 in place of u128 for u64 operations,
//! and supports U128 operations.
//!

use crate::libraries::big_num::{U128, U256, U512};

/// Trait for calculating `val * num / denom` with different rounding modes and overflow
/// protection.
///
/// Implementations of this trait have to ensure that even if the result of the multiplication does
/// not fit into the type, as long as it would fit after the division the correct result has to be
/// returned instead of `None`. `None` only should be returned if the overall result does not fit
/// into the type.
///
/// This specifically means that e.g. the `u64` implementation must, depending on the arguments, be
/// able to do 128 bit integer multiplication.
pub trait MulDiv<RHS = Self> {
    /// Output type for the methods of this trait.
    type Output;

    /// Calculates `floor(val * num / denom)`, i.e. the largest integer less than or equal to the
    /// result of the division.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use libraries::full_math::MulDiv;
    ///
    /// # fn main() {
    /// let x = 3i8.mul_div_floor(4, 2);
    /// assert_eq!(x, Some(6));
    ///
    /// let x = 5i8.mul_div_floor(2, 3);
    /// assert_eq!(x, Some(3));
    ///
    /// let x = (-5i8).mul_div_floor(2, 3);
    /// assert_eq!(x, Some(-4));
    ///
    /// let x = 3i8.mul_div_floor(3, 2);
    /// assert_eq!(x, Some(4));
    ///
    /// let x = (-3i8).mul_div_floor(3, 2);
    /// assert_eq!(x, Some(-5));
    ///
    /// let x = 127i8.mul_div_floor(4, 3);
    /// assert_eq!(x, None);
    /// # }
    /// ```
    fn mul_div_floor(self, num: RHS, denom: RHS) -> Option<Self::Output>;

    /// Calculates `ceil(val * num / denom)`, i.e. the the smallest integer greater than or equal to
    /// the result of the division.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use libraries::full_math::MulDiv;
    ///
    /// # fn main() {
    /// let x = 3i8.mul_div_ceil(4, 2);
    /// assert_eq!(x, Some(6));
    ///
    /// let x = 5i8.mul_div_ceil(2, 3);
    /// assert_eq!(x, Some(4));
    ///
    /// let x = (-5i8).mul_div_ceil(2, 3);
    /// assert_eq!(x, Some(-3));
    ///
    /// let x = 3i8.mul_div_ceil(3, 2);
    /// assert_eq!(x, Some(5));
    ///
    /// let x = (-3i8).mul_div_ceil(3, 2);
    /// assert_eq!(x, Some(-4));
    ///
    /// let x = (127i8).mul_div_ceil(4, 3);
    /// assert_eq!(x, None);
    /// # }
    /// ```
    fn mul_div_ceil(self, num: RHS, denom: RHS) -> Option<Self::Output>;

    /// Return u64 not out of bounds
    fn to_underflow_u64(self) -> u64;
}

pub trait Upcast256 {
    fn as_u256(self) -> U256;
}
impl Upcast256 for U128 {
    fn as_u256(self) -> U256 {
        U256([self.0[0], self.0[1], 0, 0])
    }
}

pub trait Downcast256 {
    /// Unsafe cast to U128
    /// Bits beyond the 128th position are lost
    fn as_u128(self) -> U128;
}
impl Downcast256 for U256 {
    fn as_u128(self) -> U128 {
        U128([self.0[0], self.0[1]])
    }
}

pub trait Upcast512 {
    fn as_u512(self) -> U512;
}
impl Upcast512 for U256 {
    fn as_u512(self) -> U512 {
        U512([self.0[0], self.0[1], self.0[2], self.0[3], 0, 0, 0, 0])
    }
}

pub trait Downcast512 {
    /// Unsafe cast to U256
    /// Bits beyond the 256th position are lost
    fn as_u256(self) -> U256;
}
impl Downcast512 for U512 {
    fn as_u256(self) -> U256 {
        U256([self.0[0], self.0[1], self.0[2], self.0[3]])
    }
}

impl MulDiv for u64 {
    type Output = u64;

    fn mul_div_floor(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, 0);
        let r = (U128::from(self) * U128::from(num)) / U128::from(denom);
        if r > U128::from(u64::MAX) {
            None
        } else {
            Some(r.as_u64())
        }
    }

    fn mul_div_ceil(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, 0);
        let r = (U128::from(self) * U128::from(num) + U128::from(denom - 1)) / U128::from(denom);
        if r > U128::from(u64::MAX) {
            None
        } else {
            Some(r.as_u64())
        }
    }

    fn to_underflow_u64(self) -> u64 {
        self
    }
}

impl MulDiv for U128 {
    type Output = U128;

    fn mul_div_floor(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, U128::default());
        let r = ((self.as_u256()) * (num.as_u256())) / (denom.as_u256());
        if r > U128::MAX.as_u256() {
            None
        } else {
            Some(r.as_u128())
        }
    }

    fn mul_div_ceil(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, U128::default());
        let r = (self.as_u256() * num.as_u256() + (denom - 1).as_u256()) / denom.as_u256();
        if r > U128::MAX.as_u256() {
            None
        } else {
            Some(r.as_u128())
        }
    }

    fn to_underflow_u64(self) -> u64 {
        if self < U128::from(u64::MAX) {
            self.as_u64()
        } else {
            0
        }
    }
}

impl MulDiv for U256 {
    type Output = U256;

    fn mul_div_floor(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, U256::default());
        let r = (self.as_u512() * num.as_u512()) / denom.as_u512();
        if r > U256::MAX.as_u512() {
            None
        } else {
            Some(r.as_u256())
        }
    }

    fn mul_div_ceil(self, num: Self, denom: Self) -> Option<Self::Output> {
        assert_ne!(denom, U256::default());
        let r = (self.as_u512() * num.as_u512() + (denom - 1).as_u512()) / denom.as_u512();
        if r > U256::MAX.as_u512() {
            None
        } else {
            Some(r.as_u256())
        }
    }

    fn to_underflow_u64(self) -> u64 {
        if self < U256::from(u64::MAX) {
            self.as_u64()
        } else {
            0
        }
    }
}

#[cfg(test)]
mod muldiv_u64_tests {
    use super::*;

    use quickcheck::{quickcheck, Arbitrary, Gen};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct NonZero(u64);

    impl Arbitrary for NonZero {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            loop {
                let v = u64::arbitrary(g);
                if v != 0 {
                    return NonZero(v);
                }
            }
        }
    }

    quickcheck! {
        fn scale_floor(val: u64, num: u64, den: NonZero) -> bool {
            let res = val.mul_div_floor(num, den.0);

            let expected = (U128::from(val) * U128::from(num)) / U128::from(den.0);

            if expected > U128::from(u64::MAX) {
                res.is_none()
            } else {
                res == Some(expected.as_u64())
            }
        }
    }

    quickcheck! {
        fn scale_ceil(val: u64, num: u64, den: NonZero) -> bool {
            let res = val.mul_div_ceil(num, den.0);

            let mut expected = (U128::from(val) * U128::from(num)) / U128::from(den.0);
            let expected_rem = (U128::from(val) * U128::from(num)) % U128::from(den.0);

            if expected_rem != U128::default() {
                expected += U128::from(1)
            }

            if expected > U128::from(u64::MAX) {
                res.is_none()
            } else {
                res == Some(expected.as_u64())
            }
        }
    }
}

#[cfg(test)]
mod muldiv_u128_tests {
    use super::*;

    use quickcheck::{quickcheck, Arbitrary, Gen};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct NonZero(U128);

    impl Arbitrary for NonZero {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            loop {
                let v = U128::from(u128::arbitrary(g));
                if v != U128::default() {
                    return NonZero(v);
                }
            }
        }
    }

    impl Arbitrary for U128 {
        fn arbitrary<G: Gen>(g: &mut G) -> Self {
            loop {
                let v = U128::from(u128::arbitrary(g));
                if v != U128::default() {
                    return v;
                }
            }
        }
    }

    quickcheck! {
        fn scale_floor(val: U128, num: U128, den: NonZero) -> bool {
            let res = val.mul_div_floor(num, den.0);

            let expected = ((val.as_u256()) * (num.as_u256())) / (den.0.as_u256());

            if expected > U128::MAX.as_u256() {
                res.is_none()
            } else {
                res == Some(expected.as_u128())
            }
        }
    }

    quickcheck! {
        fn scale_ceil(val: U128, num: U128, den: NonZero) -> bool {
            let res = val.mul_div_ceil(num, den.0);

            let mut expected = ((val.as_u256()) * (num.as_u256())) / (den.0.as_u256());
            let expected_rem = ((val.as_u256()) * (num.as_u256())) % (den.0.as_u256());

            if expected_rem != U256::default() {
                expected += U256::from(1)
            }

            if expected > U128::MAX.as_u256() {
                res.is_none()
            } else {
                res == Some(expected.as_u128())
            }
        }
    }
}
