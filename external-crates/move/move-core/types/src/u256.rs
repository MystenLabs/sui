// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use ethnum::U256 as EthnumU256;
use num::{bigint::Sign, BigInt};
#[cfg(any(test, feature = "fuzzing"))]
use proptest::strategy::BoxedStrategy;
use rand::{
    distributions::{
        uniform::{SampleUniform, UniformSampler},
        Distribution, Standard,
    },
    Rng,
};
use std::{
    fmt,
    mem::size_of,
    ops::{
        Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitXor, Div, DivAssign, Mul, MulAssign, Rem,
        RemAssign, Shl, Shr, Sub, SubAssign,
    },
};
use uint::FromStrRadixErr;

// This U256 impl was chosen for now but we are open to changing it as needed
use primitive_types::U256 as PrimitiveU256;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const NUM_BITS_PER_BYTE: usize = 8;
const U256_NUM_BITS: usize = 256;
pub const U256_NUM_BYTES: usize = U256_NUM_BITS / NUM_BITS_PER_BYTE;

#[derive(Debug)]
pub struct U256FromStrError(FromStrRadixErr);

/// A list of error categories encountered when parsing numbers.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum U256CastErrorKind {
    /// Value too large to fit in U8.
    TooLargeForU8,

    /// Value too large to fit in U16.
    TooLargeForU16,

    /// Value too large to fit in U32.
    TooLargeForU32,

    /// Value too large to fit in U64.
    TooLargeForU64,

    /// Value too large to fit in U128.
    TooLargeForU128,
}

#[derive(Debug)]
pub struct U256CastError {
    kind: U256CastErrorKind,
    val: U256,
}

impl U256CastError {
    pub fn new<T: std::convert::Into<U256>>(val: T, kind: U256CastErrorKind) -> Self {
        Self {
            kind,
            val: val.into(),
        }
    }
}

impl std::error::Error for U256CastError {}

impl fmt::Display for U256CastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = match self.kind {
            U256CastErrorKind::TooLargeForU8 => "u8",
            U256CastErrorKind::TooLargeForU16 => "u16",
            U256CastErrorKind::TooLargeForU32 => "u32",
            U256CastErrorKind::TooLargeForU64 => "u64",
            U256CastErrorKind::TooLargeForU128 => "u128",
        };
        let err_str = format!("Cast failed. {} too large for {}.", self.val, type_str);
        write!(f, "{err_str}")
    }
}

impl std::error::Error for U256FromStrError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl fmt::Display for U256FromStrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Copy, PartialOrd, Ord, Default)]
pub struct U256(PrimitiveU256);

impl fmt::Display for U256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::UpperHex for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for U256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

impl std::str::FromStr for U256 {
    type Err = U256FromStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str_radix(s, 10)
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(U256::from_le_bytes(
            &(<[u8; U256_NUM_BYTES]>::deserialize(deserializer)?),
        ))
    }
}

impl Serialize for U256 {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_le_bytes().serialize(serializer)
    }
}

impl Shl<u32> for U256 {
    type Output = Self;

    fn shl(self, rhs: u32) -> Self::Output {
        let Self(lhs) = self;
        Self(lhs << rhs)
    }
}

impl Shl<u8> for U256 {
    type Output = Self;

    fn shl(self, rhs: u8) -> Self::Output {
        let Self(lhs) = self;
        Self(lhs << rhs)
    }
}

impl Shr<u8> for U256 {
    type Output = Self;

    fn shr(self, rhs: u8) -> Self::Output {
        let Self(lhs) = self;
        Self(lhs >> rhs)
    }
}

impl BitOr<U256> for U256 {
    type Output = Self;

    fn bitor(self, rhs: U256) -> Self::Output {
        let Self(lhs) = self;
        let Self(rhs) = rhs;
        Self(lhs | rhs)
    }
}

impl BitAnd<U256> for U256 {
    type Output = Self;

    fn bitand(self, rhs: U256) -> Self::Output {
        let Self(lhs) = self;
        let Self(rhs) = rhs;
        Self(lhs & rhs)
    }
}

impl BitXor<U256> for U256 {
    type Output = Self;

    fn bitxor(self, rhs: U256) -> Self::Output {
        let Self(lhs) = self;
        let Self(rhs) = rhs;
        Self(lhs ^ rhs)
    }
}

impl BitAndAssign<U256> for U256 {
    fn bitand_assign(&mut self, rhs: U256) {
        *self = *self & rhs;
    }
}

// Ignores overflows
impl Add<U256> for U256 {
    type Output = Self;

    fn add(self, rhs: U256) -> Self::Output {
        self.wrapping_add(rhs)
    }
}

impl AddAssign<U256> for U256 {
    fn add_assign(&mut self, rhs: U256) {
        *self = *self + rhs;
    }
}

// Ignores underflows
impl Sub<U256> for U256 {
    type Output = Self;

    fn sub(self, rhs: U256) -> Self::Output {
        self.wrapping_sub(rhs)
    }
}

impl SubAssign<U256> for U256 {
    fn sub_assign(&mut self, rhs: U256) {
        *self = *self - rhs;
    }
}

// Ignores overflows
impl Mul<U256> for U256 {
    type Output = Self;

    fn mul(self, rhs: U256) -> Self::Output {
        self.wrapping_mul(rhs)
    }
}

impl MulAssign<U256> for U256 {
    fn mul_assign(&mut self, rhs: U256) {
        *self = *self * rhs;
    }
}

impl Div<U256> for U256 {
    type Output = Self;

    fn div(self, rhs: U256) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl DivAssign<U256> for U256 {
    fn div_assign(&mut self, rhs: U256) {
        *self = *self / rhs;
    }
}

impl Rem<U256> for U256 {
    type Output = Self;

    fn rem(self, rhs: U256) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

impl RemAssign<U256> for U256 {
    fn rem_assign(&mut self, rhs: U256) {
        *self = Self(self.0 % rhs.0);
    }
}

impl U256 {
    /// Zero value as U256
    pub const fn zero() -> Self {
        Self(PrimitiveU256::zero())
    }

    /// One value as U256
    pub const fn one() -> Self {
        Self(PrimitiveU256::one())
    }

    /// Max value of U256: 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
    pub const fn max_value() -> Self {
        Self(PrimitiveU256::max_value())
    }

    /// U256 from string with radix 10 or 16
    pub fn from_str_radix(src: &str, radix: u32) -> Result<Self, U256FromStrError> {
        PrimitiveU256::from_str_radix(src.trim_start_matches('0'), radix)
            .map(Self)
            .map_err(U256FromStrError)
    }

    /// U256 from 32 little endian bytes
    pub fn from_le_bytes(slice: &[u8; U256_NUM_BYTES]) -> Self {
        Self(PrimitiveU256::from_little_endian(slice))
    }

    /// U256 to 32 little endian bytes
    pub fn to_le_bytes(self) -> [u8; U256_NUM_BYTES] {
        let mut bytes = [0u8; U256_NUM_BYTES];
        self.0.to_little_endian(&mut bytes);
        bytes
    }

    /// Leading zeros of the number
    pub fn leading_zeros(&self) -> u32 {
        self.0.leading_zeros()
    }

    // Unchecked downcasting. Values as truncated if larger than target max
    pub fn unchecked_as_u8(&self) -> u8 {
        self.0.low_u128() as u8
    }

    pub fn unchecked_as_u16(&self) -> u16 {
        self.0.low_u128() as u16
    }

    pub fn unchecked_as_u32(&self) -> u32 {
        self.0.low_u128() as u32
    }

    pub fn unchecked_as_u64(&self) -> u64 {
        self.0.low_u128() as u64
    }

    pub fn unchecked_as_u128(&self) -> u128 {
        self.0.low_u128()
    }

    // Check arithmetic
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

    /// Checked integer division. Computes self / rhs, returning None if rhs == 0.
    pub fn checked_div(self, rhs: Self) -> Option<Self> {
        self.0.checked_div(rhs.0).map(Self)
    }

    /// Checked integer remainder. Computes self % rhs, returning None if rhs == 0.
    pub fn checked_rem(self, rhs: Self) -> Option<Self> {
        self.0.checked_rem(rhs.0).map(Self)
    }

    /// Checked integer remainder. Computes self % rhs, returning None if rhs == 0.
    pub fn checked_shl(self, rhs: u32) -> Option<Self> {
        if rhs >= U256_NUM_BITS as u32 {
            return None;
        }
        Some(Self(self.0.shl(rhs)))
    }

    /// Checked shift right. Computes self >> rhs, returning None if rhs is larger than or equal to the number of bits in self.
    pub fn checked_shr(self, rhs: u32) -> Option<Self> {
        if rhs >= U256_NUM_BITS as u32 {
            return None;
        }
        Some(Self(self.0.shr(rhs)))
    }

    /// Downcast to a an unsigned value of type T
    /// T must be at most u128
    pub fn down_cast_lossy<T: std::convert::TryFrom<u128>>(self) -> T {
        // Size of this type
        let type_size = size_of::<T>();
        // Maximum value for this type
        let max_val: u128 = if type_size < 16 {
            (1u128 << (NUM_BITS_PER_BYTE * type_size)) - 1u128
        } else {
            u128::MAX
        };
        // This should never fail
        match T::try_from(self.0.low_u128() & max_val) {
            Ok(w) => w,
            Err(_) => panic!("Fatal! Downcast failed"),
        }
    }

    /// Wrapping integer addition. Computes self + rhs,  wrapping around at the boundary of the type.
    /// By definition in std::instrinsics, a.wrapping_add(b) = (a + b) % (2^N), where N is bit width
    pub fn wrapping_add(self, rhs: Self) -> Self {
        Self(self.0.overflowing_add(rhs.0).0)
    }

    /// Wrapping integer subtraction. Computes self - rhs,  wrapping around at the boundary of the type.
    /// By definition in std::instrinsics, a.wrapping_add(b) = (a - b) % (2^N), where N is bit width
    pub fn wrapping_sub(self, rhs: Self) -> Self {
        Self(self.0.overflowing_sub(rhs.0).0)
    }

    /// Wrapping integer multiplication. Computes self * rhs,  wrapping around at the boundary of the type.
    /// By definition in std::instrinsics, a.wrapping_mul(b) = (a * b) % (2^N), where N is bit width
    pub fn wrapping_mul(self, rhs: Self) -> Self {
        Self(self.0.overflowing_mul(rhs.0).0)
    }

    /// Implementation of widenining multiply
    /// https://github.com/rust-random/rand/blob/master/src/distributions/utils.rs
    #[inline(always)]
    fn wmul(self, b: Self) -> (Self, Self) {
        let half = 128;
        #[allow(non_snake_case)]
        let LOWER_MASK: U256 = Self::max_value() >> half;

        let mut low = (self & LOWER_MASK).wrapping_mul(b & LOWER_MASK);
        let mut t = low >> half;
        low &= LOWER_MASK;
        t += (self >> half).wrapping_mul(b & LOWER_MASK);
        low += (t & LOWER_MASK) << half;
        let mut high = t >> half;
        t = low >> half;
        low &= LOWER_MASK;
        t += (b >> half).wrapping_mul(self & LOWER_MASK);
        low += (t & LOWER_MASK) << half;
        high += t >> half;
        high += (self >> half).wrapping_mul(b >> half);

        (high, low)
    }
}

impl From<u8> for U256 {
    fn from(n: u8) -> Self {
        U256(PrimitiveU256::from(n))
    }
}

impl From<u16> for U256 {
    fn from(n: u16) -> Self {
        U256(PrimitiveU256::from(n))
    }
}

impl From<u32> for U256 {
    fn from(n: u32) -> Self {
        U256(PrimitiveU256::from(n))
    }
}

impl From<u64> for U256 {
    fn from(n: u64) -> Self {
        U256(PrimitiveU256::from(n))
    }
}

impl From<u128> for U256 {
    fn from(n: u128) -> Self {
        U256(PrimitiveU256::from(n))
    }
}

/// TODO (ade): Remove conversions and migrate Prover & Move Model code from BigInt
impl From<&U256> for BigInt {
    fn from(n: &U256) -> Self {
        BigInt::from_bytes_le(Sign::Plus, &n.to_le_bytes())
    }
}

/// TODO (ade): Remove conversions and migrate Prover & Move Model code from EthnumU256
impl From<&U256> for EthnumU256 {
    fn from(n: &U256) -> EthnumU256 {
        // TODO (ade): use better solution for conversion
        // Currently using str because EthnumU256 can be little or big endian
        let num_str = format!("{:X}", n.0);
        // TODO (ade): remove expect()
        EthnumU256::from_str_radix(&num_str, 16).expect("Cannot convert to U256")
    }
}

impl TryFrom<U256> for u8 {
    type Error = U256CastError;
    fn try_from(n: U256) -> Result<Self, Self::Error> {
        let n = n.0.low_u64();
        if n > u8::MAX as u64 {
            Err(U256CastError::new(n, U256CastErrorKind::TooLargeForU8))
        } else {
            Ok(n as u8)
        }
    }
}

impl TryFrom<U256> for u16 {
    type Error = U256CastError;

    fn try_from(n: U256) -> Result<Self, Self::Error> {
        let n = n.0.low_u64();
        if n > u16::MAX as u64 {
            Err(U256CastError::new(n, U256CastErrorKind::TooLargeForU16))
        } else {
            Ok(n as u16)
        }
    }
}

impl TryFrom<U256> for u32 {
    type Error = U256CastError;

    fn try_from(n: U256) -> Result<Self, Self::Error> {
        let n = n.0.low_u64();
        if n > u32::MAX as u64 {
            Err(U256CastError::new(n, U256CastErrorKind::TooLargeForU32))
        } else {
            Ok(n as u32)
        }
    }
}

impl TryFrom<U256> for u64 {
    type Error = U256CastError;

    fn try_from(n: U256) -> Result<Self, Self::Error> {
        let n = n.0.low_u128();
        if n > u64::MAX as u128 {
            Err(U256CastError::new(n, U256CastErrorKind::TooLargeForU64))
        } else {
            Ok(n as u64)
        }
    }
}

impl TryFrom<U256> for u128 {
    type Error = U256CastError;

    fn try_from(n: U256) -> Result<Self, Self::Error> {
        if n > U256::from(u128::MAX) {
            Err(U256CastError::new(n, U256CastErrorKind::TooLargeForU128))
        } else {
            Ok(n.0.low_u128())
        }
    }
}

impl Distribution<U256> for Standard {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> U256 {
        let mut dest = [0; U256_NUM_BYTES];
        rng.fill_bytes(&mut dest);
        U256::from_le_bytes(&dest)
    }
}

// Rand impl below are inspired by u128 impl found in https://rust-random.github.io/rand/src/rand/distributions/uniform.rs.html

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde1", derive(Serialize, Deserialize))]
pub struct UniformU256 {
    low: U256,
    range: U256,
    z: U256,
}

impl SampleUniform for U256 {
    type Sampler = UniformU256;
}

impl UniformSampler for UniformU256 {
    type X = U256;

    fn new<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low.borrow();
        let high = *high.borrow();
        assert!(low < high, "Uniform::new called with `low >= high`");
        UniformSampler::new_inclusive(low, high - U256::one())
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
        let unsigned_max = U256::max_value();

        let range = high.wrapping_sub(low).wrapping_add(U256::one());

        let ints_to_reject = if range > U256::zero() {
            (unsigned_max - range) + U256::one() % range
        } else {
            U256::zero()
        };

        UniformU256 {
            low,
            range,
            z: ints_to_reject,
        }
    }

    fn sample<R: rand::Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        let range = self.range;
        if range > U256::zero() {
            let unsigned_max = U256::max_value();
            let zone = unsigned_max - self.z;
            loop {
                let v: U256 = rng.gen();
                let (hi, lo) = v.wmul(range);
                if lo <= zone {
                    return self.low.wrapping_add(hi);
                }
            }
        } else {
            // Sample from the entire integer range.
            rng.gen()
        }
    }

    fn sample_single<R: rand::Rng + ?Sized, B1, B2>(low: B1, high: B2, rng: &mut R) -> Self::X
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = *low.borrow();
        let high = *high.borrow();
        assert!(low < high, "UniformSampler::sample_single: low >= high");
        Self::sample_single_inclusive(low, high - U256::one(), rng)
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
        let low = *low.borrow();
        let high = *high.borrow();
        assert!(
            low <= high,
            "UniformSampler::sample_single_inclusive: low > high"
        );
        let range = high.wrapping_sub(low).wrapping_add(U256::one());
        // If the above resulted in wrap-around to 0, the range is U256::MIN..=U256::MAX,
        // and any integer will do.
        if range == U256::zero() {
            return rng.gen();
        }
        // conservative but fast approximation. `- 1` is necessary to allow the
        // same comparison without bias.
        let zone = (range << range.leading_zeros()).wrapping_sub(U256::one());

        loop {
            let v: U256 = rng.gen();
            let (hi, lo) = v.wmul(range);
            if lo <= zone {
                return low.wrapping_add(hi);
            }
        }
    }
}

#[cfg(any(test, feature = "fuzzing"))]
impl proptest::prelude::Arbitrary for U256 {
    type Strategy = BoxedStrategy<Self>;
    type Parameters = ();
    fn arbitrary_with(_params: Self::Parameters) -> Self::Strategy {
        use proptest::strategy::Strategy as _;
        proptest::arbitrary::any::<[u8; U256_NUM_BYTES]>()
            .prop_map(|q| U256::from_le_bytes(&q))
            .boxed()
    }
}

#[cfg(any(test, feature = "fuzzing"))]
impl<'a> arbitrary::Arbitrary<'a> for U256 {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let bytes = <[u8; U256_NUM_BYTES]>::arbitrary(u)?;
        Ok(U256::from_le_bytes(&bytes))
    }
}

#[test]
fn wrapping_add() {
    // a + b overflows U256::MAX by 100
    // By definition in std::instrinsics, a.wrapping_add(b) = (a + b) % (2^N), where N is bit width

    let a = U256::from(1234u32);
    let b = U256::from_str_radix(
        "115792089237316195423570985008687907853269984665640564039457584007913129638801",
        10,
    )
    .unwrap();

    assert!(a.wrapping_add(b) == U256::from(99u8));
}
