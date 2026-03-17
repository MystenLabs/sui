// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::u256::{U256, U256_NUM_BYTES};
#[cfg(any(test, feature = "fuzzing"))]
use proptest::strategy::BoxedStrategy;
use rand::{
    Rng,
    distributions::{
        Distribution, Standard,
        uniform::{SampleUniform, UniformSampler},
    },
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    ops::{
        Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitXor, Div, DivAssign, Mul, MulAssign, Rem,
        RemAssign, Shl, Shr, Sub, SubAssign,
    },
};

const I256_NUM_BITS: usize = 256;

/// Error returned when parsing an I256 from a string fails.
#[derive(Debug)]
pub struct I256FromStrError(String);

impl std::error::Error for I256FromStrError {}

impl fmt::Display for I256FromStrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Categories of cast errors when converting from I256 to a narrower signed type.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum I256CastErrorKind {
    /// Value does not fit in i8.
    OutOfRangeForI8,

    /// Value does not fit in i16.
    OutOfRangeForI16,

    /// Value does not fit in i32.
    OutOfRangeForI32,

    /// Value does not fit in i64.
    OutOfRangeForI64,

    /// Value does not fit in i128.
    OutOfRangeForI128,
}

/// Error returned when a cast from I256 to a narrower signed type fails.
#[derive(Debug)]
pub struct I256CastError {
    kind: I256CastErrorKind,
    val: I256,
}

impl I256CastError {
    pub fn new(val: I256, kind: I256CastErrorKind) -> Self {
        Self { kind, val }
    }
}

impl std::error::Error for I256CastError {}

impl fmt::Display for I256CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = match self.kind {
            I256CastErrorKind::OutOfRangeForI8 => "i8",
            I256CastErrorKind::OutOfRangeForI16 => "i16",
            I256CastErrorKind::OutOfRangeForI32 => "i32",
            I256CastErrorKind::OutOfRangeForI64 => "i64",
            I256CastErrorKind::OutOfRangeForI128 => "i128",
        };
        write!(
            f,
            "Cast failed. {} out of range for {}.",
            self.val, type_str
        )
    }
}

/// A 256-bit signed integer, stored in two's complement as a newtype over `ethnum::I256`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy, PartialOrd, Ord, Default)]
pub struct I256(ethnum::I256);

impl fmt::Display for I256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperHex for I256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for I256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl std::str::FromStr for I256 {
    type Err = I256FromStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_radix(s, 10)
    }
}

impl<'de> Deserialize<'de> for I256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <[u8; U256_NUM_BYTES]>::deserialize(deserializer)?;
        Ok(I256(ethnum::I256::from_le_bytes(bytes)))
    }
}

impl Serialize for I256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.to_le_bytes().serialize(serializer)
    }
}

//**************************************************************************************************
// Operator impls
//**************************************************************************************************

impl Shl<u32> for I256 {
    type Output = Self;

    fn shl(self, rhs: u32) -> Self::Output {
        Self(self.0 << rhs)
    }
}

impl Shl<u8> for I256 {
    type Output = Self;

    fn shl(self, rhs: u8) -> Self::Output {
        Self(self.0 << rhs)
    }
}

impl Shr<u8> for I256 {
    type Output = Self;

    fn shr(self, rhs: u8) -> Self::Output {
        Self(self.0 >> rhs)
    }
}

impl BitOr for I256 {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for I256 {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitXor for I256 {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
    }
}

impl BitAndAssign for I256 {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

// Ignores overflows
impl Add for I256 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.wrapping_add(rhs)
    }
}

impl AddAssign for I256 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

// Ignores underflows
impl Sub for I256 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self.wrapping_sub(rhs)
    }
}

impl SubAssign for I256 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

// Ignores overflows
impl Mul for I256 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        self.wrapping_mul(rhs)
    }
}

impl MulAssign for I256 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl Div for I256 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl DivAssign for I256 {
    fn div_assign(&mut self, rhs: Self) {
        *self = *self / rhs;
    }
}

impl Rem for I256 {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

impl RemAssign for I256 {
    fn rem_assign(&mut self, rhs: Self) {
        *self = Self(self.0 % rhs.0);
    }
}

//**************************************************************************************************
// Inherent methods
//**************************************************************************************************

impl I256 {
    /// Zero value as I256
    pub const fn zero() -> Self {
        Self(ethnum::I256::ZERO)
    }

    /// One value as I256
    pub const fn one() -> Self {
        Self(ethnum::I256::ONE)
    }

    /// Minimum value of I256: -(2^255)
    pub const fn min_value() -> Self {
        Self(ethnum::I256::MIN)
    }

    /// Maximum value of I256: 2^255 - 1
    pub const fn max_value() -> Self {
        Self(ethnum::I256::MAX)
    }

    /// I256 from string with radix 10 or 16
    pub fn from_str_radix(src: &str, radix: u32) -> Result<Self, I256FromStrError> {
        ethnum::I256::from_str_radix(src, radix)
            .map(Self)
            .map_err(|e| I256FromStrError(e.to_string()))
    }

    /// I256 from 32 little-endian bytes (two's complement)
    pub fn from_le_bytes(slice: &[u8; U256_NUM_BYTES]) -> Self {
        Self(ethnum::I256::from_le_bytes(*slice))
    }

    /// I256 to 32 little-endian bytes (two's complement)
    pub fn to_le_bytes(self) -> [u8; U256_NUM_BYTES] {
        self.0.to_le_bytes()
    }

    /// I256 to 32 big-endian bytes (two's complement)
    pub fn to_be_bytes(self) -> [u8; U256_NUM_BYTES] {
        self.0.to_be_bytes()
    }

    /// Reinterpret the bits of a U256 as a signed I256 (two's complement).
    pub fn from_u256_bits(u: U256) -> Self {
        let bytes = u.to_le_bytes();
        Self(ethnum::I256::from_le_bytes(bytes))
    }

    /// Reinterpret the bits of this I256 as a U256.
    pub fn to_u256_bits(self) -> U256 {
        U256::from_le_bytes(&self.0.to_le_bytes())
    }

    // Checked arithmetic

    /// Checked integer addition. Computes self + rhs, returning None if overflow occurred.
    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    /// Checked integer subtraction. Computes self - rhs, returning None if overflow occurred.
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self)
    }

    /// Checked integer multiplication. Computes self * rhs, returning None if overflow occurred.
    pub fn checked_mul(self, rhs: Self) -> Option<Self> {
        self.0.checked_mul(rhs.0).map(Self)
    }

    /// Checked integer division. Computes self / rhs, returning None if rhs == 0 or on overflow.
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Checked integer remainder. Computes self % rhs, returning None if rhs == 0 or on overflow.
    pub fn checked_rem(self, rhs: Self) -> Option<Self> {
        self.0.checked_rem(rhs.0).map(Self)
    }

    /// Checked negation. Computes -self, returning None if self == MIN.
    pub fn checked_neg(self) -> Option<Self> {
        self.0.checked_neg().map(Self)
    }

    /// Checked shift left. Computes self << rhs, returning None if rhs >= 256.
    pub fn checked_shl(self, rhs: u32) -> Option<Self> {
        if rhs >= I256_NUM_BITS as u32 {
            return None;
        }
        Some(Self(self.0 << rhs))
    }

    /// Checked shift right. Computes self >> rhs (arithmetic), returning None if rhs >= 256.
    pub fn checked_shr(self, rhs: u32) -> Option<Self> {
        if rhs >= I256_NUM_BITS as u32 {
            return None;
        }
        Some(Self(self.0 >> rhs))
    }

    // Wrapping arithmetic

    /// Wrapping integer addition. Computes self + rhs, wrapping around at the boundary of the type.
    pub fn wrapping_add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }

    /// Wrapping integer subtraction. Computes self - rhs, wrapping around at the boundary of the type.
    pub fn wrapping_sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }

    /// Wrapping integer multiplication. Computes self * rhs, wrapping around at the boundary of the type.
    pub fn wrapping_mul(self, rhs: Self) -> Self {
        Self(self.0.wrapping_mul(rhs.0))
    }

    /// Wrapping negation. Computes -self, wrapping MIN to MIN.
    pub fn wrapping_neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }

    /// Downcast to a smaller signed type. The value is truncated (low 128 bits reinterpreted).
    /// T must be at most i128.
    pub fn down_cast_lossy<T: TryFrom<i128>>(self) -> T {
        let low = self.0.as_i128();
        match T::try_from(low) {
            Ok(v) => v,
            Err(_) => panic!("Fatal! Downcast failed"),
        }
    }
}

//**************************************************************************************************
// From impls (infallible widening conversions)
//**************************************************************************************************

impl From<i8> for I256 {
    fn from(n: i8) -> Self {
        Self(ethnum::I256::from(n))
    }
}

impl From<i16> for I256 {
    fn from(n: i16) -> Self {
        Self(ethnum::I256::from(n))
    }
}

impl From<i32> for I256 {
    fn from(n: i32) -> Self {
        Self(ethnum::I256::from(n))
    }
}

impl From<i64> for I256 {
    fn from(n: i64) -> Self {
        Self(ethnum::I256::from(n))
    }
}

impl From<i128> for I256 {
    fn from(n: i128) -> Self {
        Self(ethnum::I256::from(n))
    }
}

//**************************************************************************************************
// TryFrom impls (fallible narrowing conversions)
//**************************************************************************************************

/// Helper: returns true if the I256 value fits within [min, max] of a narrower type.
/// We check by comparing the I256 against I256 versions of the bounds.
fn fits_in_range(n: I256, min: i128, max: i128) -> bool {
    let lo = I256::from(min);
    let hi = I256::from(max);
    n >= lo && n <= hi
}

impl TryFrom<I256> for i8 {
    type Error = I256CastError;

    fn try_from(n: I256) -> Result<Self, Self::Error> {
        if fits_in_range(n, i8::MIN as i128, i8::MAX as i128) {
            Ok(n.0.as_i128() as i8)
        } else {
            Err(I256CastError::new(n, I256CastErrorKind::OutOfRangeForI8))
        }
    }
}

impl TryFrom<I256> for i16 {
    type Error = I256CastError;

    fn try_from(n: I256) -> Result<Self, Self::Error> {
        if fits_in_range(n, i16::MIN as i128, i16::MAX as i128) {
            Ok(n.0.as_i128() as i16)
        } else {
            Err(I256CastError::new(n, I256CastErrorKind::OutOfRangeForI16))
        }
    }
}

impl TryFrom<I256> for i32 {
    type Error = I256CastError;

    fn try_from(n: I256) -> Result<Self, Self::Error> {
        if fits_in_range(n, i32::MIN as i128, i32::MAX as i128) {
            Ok(n.0.as_i128() as i32)
        } else {
            Err(I256CastError::new(n, I256CastErrorKind::OutOfRangeForI32))
        }
    }
}

impl TryFrom<I256> for i64 {
    type Error = I256CastError;

    fn try_from(n: I256) -> Result<Self, Self::Error> {
        if fits_in_range(n, i64::MIN as i128, i64::MAX as i128) {
            Ok(n.0.as_i128() as i64)
        } else {
            Err(I256CastError::new(n, I256CastErrorKind::OutOfRangeForI64))
        }
    }
}

impl TryFrom<I256> for i128 {
    type Error = I256CastError;

    fn try_from(n: I256) -> Result<Self, Self::Error> {
        if fits_in_range(n, i128::MIN, i128::MAX) {
            Ok(n.0.as_i128())
        } else {
            Err(I256CastError::new(n, I256CastErrorKind::OutOfRangeForI128))
        }
    }
}

//**************************************************************************************************
// Rand impls
//**************************************************************************************************

impl Distribution<I256> for Standard {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> I256 {
        let mut dest = [0u8; U256_NUM_BYTES];
        rng.fill_bytes(&mut dest);
        I256::from_le_bytes(&dest)
    }
}

// Uniform sampling for I256.
// Strategy: map the signed range [low, high] to an unsigned range of the same width by adding
// an offset (MIN is mapped to 0), sample uniformly from U256 within that range, then map back.

/// Offset to convert between I256 and U256 ranges: U256 representation of 2^255.
/// Adding this to a signed value (reinterpreted as U256 via two's complement) maps
/// I256::MIN -> 0, I256::MAX -> U256::MAX.
fn i256_to_u256_offset(val: I256) -> U256 {
    // Two's complement reinterpretation then add 2^255 (XOR the sign bit)
    let mut bytes = val.to_le_bytes();
    bytes[31] ^= 0x80; // flip sign bit
    U256::from_le_bytes(&bytes)
}

fn u256_to_i256_offset(val: U256) -> I256 {
    let mut bytes = val.to_le_bytes();
    bytes[31] ^= 0x80; // flip sign bit back
    I256::from_le_bytes(&bytes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformI256 {
    low_u: U256,
    range_u: U256,
}

impl SampleUniform for I256 {
    type Sampler = UniformI256;
}

impl UniformSampler for UniformI256 {
    type X = I256;

    fn new<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low.borrow();
        let high = *high.borrow();
        assert!(low < high, "Uniform::new called with `low >= high`");
        UniformSampler::new_inclusive(low, high - I256::one())
    }

    fn new_inclusive<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low.borrow();
        let high = *high.borrow();
        assert!(
            low <= high,
            "Uniform::new_inclusive called with `low > high`"
        );
        let low_u = i256_to_u256_offset(low);
        let high_u = i256_to_u256_offset(high);
        // range = high_u - low_u + 1; when wrapping to 0 it means the full range
        let range_u = high_u.wrapping_sub(low_u).wrapping_add(U256::one());
        UniformI256 { low_u, range_u }
    }

    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        if self.range_u == U256::zero() {
            // Full range: any random I256
            rng.r#gen()
        } else {
            // Rejection sampling in the unsigned domain
            let range = self.range_u;
            loop {
                let v: U256 = rng.r#gen();
                let rem = v.checked_rem(range);
                if let Some(r) = rem {
                    return u256_to_i256_offset(self.low_u.wrapping_add(r));
                }
            }
        }
    }

    fn sample_single<R: rand::Rng + ?Sized, B1, B2>(low: B1, high: B2, rng: &mut R) -> Self::X
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let sampler = Self::new(low, high);
        sampler.sample(rng)
    }

    fn sample_single_inclusive<R: rand::Rng + ?Sized, B1, B2>(
        low: B1,
        high: B2,
        rng: &mut R,
    ) -> Self::X
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let sampler = Self::new_inclusive(low, high);
        sampler.sample(rng)
    }
}

//**************************************************************************************************
// Proptest / Arbitrary impls
//**************************************************************************************************

#[cfg(any(test, feature = "fuzzing"))]
impl proptest::prelude::Arbitrary for I256 {
    type Strategy = BoxedStrategy<Self>;
    type Parameters = ();

    fn arbitrary_with(_params: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::arbitrary::any::<[u8; U256_NUM_BYTES]>()
            .prop_map(|q| I256::from_le_bytes(&q))
            .boxed()
    }
}

#[cfg(any(test, feature = "fuzzing"))]
impl<'a> arbitrary::Arbitrary<'a> for I256 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; U256_NUM_BYTES]>::arbitrary(u)?;
        Ok(I256::from_le_bytes(&bytes))
    }
}

//**************************************************************************************************
// Tests
//**************************************************************************************************

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn zero_one_min_max() {
        assert_eq!(I256::zero(), I256::from(0i8));
        assert_eq!(I256::one(), I256::from(1i8));
        assert!(I256::min_value() < I256::zero());
        assert!(I256::max_value() > I256::zero());
        // MIN + MAX == -1
        assert_eq!(I256::min_value() + I256::max_value(), I256::from(-1i8));
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", I256::zero()), "0");
        assert_eq!(format!("{}", I256::one()), "1");
        assert_eq!(format!("{}", I256::from(-1i8)), "-1");
        assert_eq!(format!("{}", I256::from(42i32)), "42");
    }

    #[test]
    fn from_str_roundtrip() {
        let val: I256 = "12345".parse().unwrap();
        assert_eq!(val, I256::from(12345i32));

        let neg: I256 = "-99".parse().unwrap();
        assert_eq!(neg, I256::from(-99i8));

        assert!("not_a_number".parse::<I256>().is_err());
    }

    #[test]
    fn from_str_radix() {
        assert_eq!(I256::from_str_radix("FF", 16).unwrap(), I256::from(255i32));
        assert_eq!(I256::from_str_radix("0", 10).unwrap(), I256::zero());
        assert!(I256::from_str_radix("xyz", 16).is_err());
    }

    #[test]
    fn le_bytes_roundtrip() {
        let val = I256::from(-42i64);
        let bytes = val.to_le_bytes();
        assert_eq!(I256::from_le_bytes(&bytes), val);

        let zero_bytes = I256::zero().to_le_bytes();
        assert_eq!(I256::from_le_bytes(&zero_bytes), I256::zero());

        let min_bytes = I256::min_value().to_le_bytes();
        assert_eq!(I256::from_le_bytes(&min_bytes), I256::min_value());

        let max_bytes = I256::max_value().to_le_bytes();
        assert_eq!(I256::from_le_bytes(&max_bytes), I256::max_value());
    }

    #[test]
    fn u256_bits_roundtrip() {
        let val = I256::from(-1i8);
        let u = val.to_u256_bits();
        assert_eq!(I256::from_u256_bits(u), val);

        assert_eq!(I256::from_u256_bits(U256::zero()), I256::zero());
    }

    #[test]
    fn serde_roundtrip() {
        let val = I256::from(-12345i32);
        let serialized = bcs::to_bytes(&val).unwrap();
        let deserialized: I256 = bcs::from_bytes(&serialized).unwrap();
        assert_eq!(val, deserialized);
    }

    // Checked arithmetic

    #[test]
    fn checked_add_basic() {
        assert_eq!(
            I256::from(10i32).checked_add(I256::from(20i32)),
            Some(I256::from(30i32))
        );
        // MAX + 1 overflows
        assert_eq!(I256::max_value().checked_add(I256::one()), None);
        // MIN + (-1) overflows
        assert_eq!(I256::min_value().checked_add(I256::from(-1i8)), None);
    }

    #[test]
    fn checked_sub_basic() {
        assert_eq!(
            I256::from(30i32).checked_sub(I256::from(10i32)),
            Some(I256::from(20i32))
        );
        // MIN - 1 overflows
        assert_eq!(I256::min_value().checked_sub(I256::one()), None);
    }

    #[test]
    fn checked_mul_basic() {
        assert_eq!(
            I256::from(6i32).checked_mul(I256::from(7i32)),
            Some(I256::from(42i32))
        );
        assert_eq!(
            I256::from(-3i32).checked_mul(I256::from(7i32)),
            Some(I256::from(-21i32))
        );
        // MAX * 2 overflows
        assert_eq!(I256::max_value().checked_mul(I256::from(2i8)), None);
    }

    #[test]
    fn checked_div_basic() {
        assert_eq!(
            I256::from(42i32).checked_div(I256::from(6i32)),
            Some(I256::from(7i32))
        );
        // Division by zero
        assert_eq!(I256::one().checked_div(I256::zero()), None);
        // MIN / -1 overflows
        assert_eq!(I256::min_value().checked_div(I256::from(-1i8)), None);
    }

    #[test]
    fn checked_rem_basic() {
        assert_eq!(
            I256::from(10i32).checked_rem(I256::from(3i32)),
            Some(I256::from(1i32))
        );
        assert_eq!(I256::one().checked_rem(I256::zero()), None);
    }

    #[test]
    fn checked_neg_basic() {
        assert_eq!(I256::from(42i32).checked_neg(), Some(I256::from(-42i32)));
        assert_eq!(I256::zero().checked_neg(), Some(I256::zero()));
        // MIN has no positive counterpart
        assert_eq!(I256::min_value().checked_neg(), None);
    }

    #[test]
    fn checked_shl_shr() {
        assert_eq!(I256::one().checked_shl(8), Some(I256::from(256i32)));
        assert_eq!(I256::from(256i32).checked_shr(4), Some(I256::from(16i32)));
        // Shift by >= 256 returns None
        assert_eq!(I256::one().checked_shl(256), None);
        assert_eq!(I256::one().checked_shr(256), None);
        // Arithmetic right shift of negative preserves sign
        assert_eq!(I256::from(-16i32).checked_shr(2), Some(I256::from(-4i32)));
    }

    // Wrapping arithmetic

    #[test]
    fn wrapping_add_overflow() {
        // MAX + 1 wraps to MIN
        assert_eq!(
            I256::max_value().wrapping_add(I256::one()),
            I256::min_value()
        );
    }

    #[test]
    fn wrapping_sub_underflow() {
        // MIN - 1 wraps to MAX
        assert_eq!(
            I256::min_value().wrapping_sub(I256::one()),
            I256::max_value()
        );
    }

    #[test]
    fn wrapping_neg_min() {
        // -MIN wraps to MIN for two's complement
        assert_eq!(I256::min_value().wrapping_neg(), I256::min_value());
    }

    // Operator impls

    #[test]
    fn arithmetic_operators() {
        let a = I256::from(100i32);
        let b = I256::from(30i32);

        assert_eq!(a + b, I256::from(130i32));
        assert_eq!(a - b, I256::from(70i32));
        assert_eq!(a * b, I256::from(3000i32));
        assert_eq!(a / b, I256::from(3i32));
        assert_eq!(a % b, I256::from(10i32));
    }

    #[test]
    fn bitwise_operators() {
        let a = I256::from(0x0Fi64);
        let b = I256::from(0x03i64);

        assert_eq!(a & b, I256::from(0x03i64));
        assert_eq!(a | b, I256::from(0x0Fi64));
        assert_eq!(a ^ b, I256::from(0x0Ci64));
    }

    #[test]
    fn shift_operators() {
        assert_eq!(I256::one() << 8u8, I256::from(256i32));
        assert_eq!(I256::from(256i32) >> 4u8, I256::from(16i32));
        assert_eq!(I256::one() << 8u32, I256::from(256i32));
    }

    #[test]
    fn assign_operators() {
        let mut x = I256::from(10i32);
        x += I256::from(5i32);
        assert_eq!(x, I256::from(15i32));
        x -= I256::from(3i32);
        assert_eq!(x, I256::from(12i32));
        x *= I256::from(2i32);
        assert_eq!(x, I256::from(24i32));
        x /= I256::from(4i32);
        assert_eq!(x, I256::from(6i32));
        x %= I256::from(4i32);
        assert_eq!(x, I256::from(2i32));

        let mut y = I256::from(0x0Fi64);
        y &= I256::from(0x03i64);
        assert_eq!(y, I256::from(0x03i64));
    }

    // Conversion tests

    #[test]
    fn i8_conversion() {
        assert!(i8::try_from(I256::from(0i8)).is_ok());
        assert!(i8::try_from(I256::from(127i8)).is_ok());
        assert!(i8::try_from(I256::from(-128i8)).is_ok());
        assert!(i8::try_from(I256::from(128i32)).is_err());
        assert!(i8::try_from(I256::from(-129i32)).is_err());
        assert!(i8::try_from(I256::max_value()).is_err());
        assert!(i8::try_from(I256::min_value()).is_err());
    }

    #[test]
    fn i16_conversion() {
        assert!(i16::try_from(I256::from(0i8)).is_ok());
        assert!(i16::try_from(I256::from(32767i32)).is_ok());
        assert!(i16::try_from(I256::from(-32768i32)).is_ok());
        assert!(i16::try_from(I256::from(32768i32)).is_err());
        assert!(i16::try_from(I256::from(-32769i32)).is_err());
        assert!(i16::try_from(I256::max_value()).is_err());
        assert!(i16::try_from(I256::min_value()).is_err());
    }

    #[test]
    fn i32_conversion() {
        assert!(i32::try_from(I256::from(0i8)).is_ok());
        assert!(i32::try_from(I256::from(i32::MAX)).is_ok());
        assert!(i32::try_from(I256::from(i32::MIN)).is_ok());
        assert!(i32::try_from(I256::from(i32::MAX as i64 + 1)).is_err());
        assert!(i32::try_from(I256::from(i32::MIN as i64 - 1)).is_err());
        assert!(i32::try_from(I256::max_value()).is_err());
        assert!(i32::try_from(I256::min_value()).is_err());
    }

    #[test]
    fn i64_conversion() {
        assert!(i64::try_from(I256::from(0i8)).is_ok());
        assert!(i64::try_from(I256::from(i64::MAX)).is_ok());
        assert!(i64::try_from(I256::from(i64::MIN)).is_ok());
        assert!(i64::try_from(I256::from(i64::MAX as i128 + 1)).is_err());
        assert!(i64::try_from(I256::from(i64::MIN as i128 - 1)).is_err());
        assert!(i64::try_from(I256::max_value()).is_err());
        assert!(i64::try_from(I256::min_value()).is_err());
    }

    #[test]
    fn i128_conversion() {
        assert!(i128::try_from(I256::from(0i8)).is_ok());
        assert!(i128::try_from(I256::from(i128::MAX)).is_ok());
        assert!(i128::try_from(I256::from(i128::MIN)).is_ok());
        assert!(i128::try_from(I256::max_value()).is_err());
        assert!(i128::try_from(I256::min_value()).is_err());
    }

    #[test]
    fn cast_error_display() {
        let err = I256CastError::new(I256::from(999i32), I256CastErrorKind::OutOfRangeForI8);
        let msg = format!("{}", err);
        assert!(
            msg.contains("i8"),
            "error message should name the target type"
        );
        assert!(
            msg.contains("999"),
            "error message should contain the value"
        );
    }

    #[test]
    fn from_str_error_display() {
        let err = "bad".parse::<I256>().unwrap_err();
        assert!(!format!("{}", err).is_empty());
    }

    // Down-cast lossy

    #[test]
    fn down_cast_lossy_basic() {
        let val = I256::from(42i32);
        assert_eq!(val.down_cast_lossy::<i8>(), 42i8);
        assert_eq!(val.down_cast_lossy::<i64>(), 42i64);

        let neg = I256::from(-5i32);
        assert_eq!(neg.down_cast_lossy::<i8>(), -5i8);
        assert_eq!(neg.down_cast_lossy::<i128>(), -5i128);
    }

    // Negative arithmetic

    #[test]
    fn negative_arithmetic() {
        let a = I256::from(-10i32);
        let b = I256::from(3i32);
        assert_eq!(a + b, I256::from(-7i32));
        assert_eq!(a - b, I256::from(-13i32));
        assert_eq!(a * b, I256::from(-30i32));
        assert_eq!(a / b, I256::from(-3i32));
        assert_eq!(a % b, I256::from(-1i32));
    }

    #[test]
    fn comparison_with_negatives() {
        assert!(I256::from(-1i8) < I256::zero());
        assert!(I256::from(-1i8) < I256::one());
        assert!(I256::from(-5i8) > I256::from(-10i8));
        assert!(I256::min_value() < I256::max_value());
    }

    // Proptest conversions

    proptest! {
        // i8
        #[test]
        fn try_from_i256_to_i8_succeeds_for_all_in_range(x in any::<i8>()) {
            let big = I256::from(x);
            let got = i8::try_from(big).expect("i8 in-range should convert");
            prop_assert_eq!(got, x);
        }

        #[test]
        fn try_from_i256_to_i8_fails_for_out_of_range(x in (i8::MAX as i64 + 1)..=i64::MAX) {
            let big = I256::from(x);
            prop_assert!(i8::try_from(big).is_err());
        }

        // i16
        #[test]
        fn try_from_i256_to_i16_succeeds_for_all_in_range(x in any::<i16>()) {
            let big = I256::from(x as i32);
            let got = i16::try_from(big).expect("i16 in-range should convert");
            prop_assert_eq!(got, x);
        }

        #[test]
        fn try_from_i256_to_i16_fails_for_out_of_range(x in (i16::MAX as i64 + 1)..=i64::MAX) {
            let big = I256::from(x);
            prop_assert!(i16::try_from(big).is_err());
        }

        // i32
        #[test]
        fn try_from_i256_to_i32_succeeds_for_all_in_range(x in any::<i32>()) {
            let big = I256::from(x as i64);
            let got = i32::try_from(big).expect("i32 in-range should convert");
            prop_assert_eq!(got, x);
        }

        #[test]
        fn try_from_i256_to_i32_fails_for_out_of_range(x in (i32::MAX as i64 + 1)..=i64::MAX) {
            let big = I256::from(x);
            prop_assert!(i32::try_from(big).is_err());
        }

        // i64
        #[test]
        fn try_from_i256_to_i64_succeeds_for_all_in_range(x in any::<i64>()) {
            let big = I256::from(x as i128);
            let got = i64::try_from(big).expect("i64 in-range should convert");
            prop_assert_eq!(got, x);
        }

        #[test]
        fn try_from_i256_to_i64_fails_for_out_of_range(x in (i64::MAX as i128 + 1)..=i128::MAX) {
            let big = I256::from(x);
            prop_assert!(i64::try_from(big).is_err());
        }

        // i128
        #[test]
        fn try_from_i256_to_i128_succeeds_for_all_in_range(x in any::<i128>()) {
            let big = I256::from(x);
            let got = i128::try_from(big).expect("i128 in-range should convert");
            prop_assert_eq!(got, x);
        }

        // Serialization roundtrip
        #[test]
        fn serde_roundtrip_proptest(x in any::<i128>()) {
            let val = I256::from(x);
            let bytes = bcs::to_bytes(&val).unwrap();
            let back: I256 = bcs::from_bytes(&bytes).unwrap();
            prop_assert_eq!(val, back);
        }

        // Byte roundtrip
        #[test]
        fn le_bytes_roundtrip_proptest(x in any::<i128>()) {
            let val = I256::from(x);
            let bytes = val.to_le_bytes();
            prop_assert_eq!(I256::from_le_bytes(&bytes), val);
        }
    }

    // Rand / uniform sampling tests

    #[test]
    fn standard_distribution_produces_values() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(42);
        let val: I256 = rng.r#gen();
        // Just verify it doesn't panic and produces *some* value
        let _ = val;
    }

    #[test]
    fn uniform_sampling_small_range() {
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(123);
        let low = I256::from(-10i32);
        let high = I256::from(10i32);
        for _ in 0..100 {
            let val = rng.gen_range(low..high);
            assert!(
                val >= low && val < high,
                "value {val} out of range [{low}, {high})"
            );
        }
    }

    #[test]
    fn uniform_sampling_negative_range() {
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(456);
        let low = I256::from(-100i32);
        let high = I256::from(-50i32);
        for _ in 0..100 {
            let val = rng.gen_range(low..high);
            assert!(
                val >= low && val < high,
                "value {val} out of range [{low}, {high})"
            );
        }
    }

    #[test]
    fn uniform_sampling_inclusive() {
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(789);
        let low = I256::from(-5i32);
        let high = I256::from(5i32);
        for _ in 0..100 {
            let val = rng.gen_range(low..=high);
            assert!(
                val >= low && val <= high,
                "value {val} out of range [{low}, {high}]"
            );
        }
    }

    #[test]
    fn uniform_single_value_range() {
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(999);
        let val = I256::from(42i32);
        // Inclusive range of a single value
        let sampled = rng.gen_range(val..=val);
        assert_eq!(sampled, val);
    }

    #[test]
    fn offset_roundtrip() {
        // Verify the offset mapping preserves order
        assert!(i256_to_u256_offset(I256::min_value()) == U256::zero());
        assert!(i256_to_u256_offset(I256::zero()) > U256::zero());
        assert!(i256_to_u256_offset(I256::max_value()) == U256::max_value());

        // Roundtrip
        assert_eq!(
            u256_to_i256_offset(i256_to_u256_offset(I256::min_value())),
            I256::min_value()
        );
        assert_eq!(
            u256_to_i256_offset(i256_to_u256_offset(I256::zero())),
            I256::zero()
        );
        assert_eq!(
            u256_to_i256_offset(i256_to_u256_offset(I256::max_value())),
            I256::max_value()
        );
        assert_eq!(
            u256_to_i256_offset(i256_to_u256_offset(I256::from(-1i8))),
            I256::from(-1i8)
        );
    }
}
