// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::{
    cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd},
    convert::From,
    fmt::{self, Debug, Display},
    marker::PhantomData,
    ops::{Add, AddAssign, Mul},
};

// TODO(Gas): deprecate the concept of abstract memory size

/***************************************************************************************************
 * Units of Measurement
 *
 **************************************************************************************************/
/// Unit of internal gas.
pub enum InternalGasUnit {}

/// Unit for counting bytes.
pub enum Byte {}

/// Alternative unit for counting bytes. 1 kibibyte = 1024 bytes.
pub enum KibiByte {}

/// Alternative unit for counting bytes. 1 mebibyte = 1024 kibibytes.
pub enum MebiByte {}

/// Alternative unit for counting bytes. 1 gibibyte = 1024 mebibytes.
pub enum GibiByte {}

/// Unit of abstract memory usage in the Move VM.
pub enum AbstractMemoryUnit {}

/// Unit for counting arguments.
pub enum Arg {}

/// A derived unit resulted from the division of two given units.
/// This is used to permit type-safe multiplications.
///
/// A real life example: 3 gas units/byte * 5 bytes = 15 gas units
pub struct UnitDiv<U1, U2> {
    phantom: PhantomData<(U1, U2)>,
}

/***************************************************************************************************
 * Gas Quantities
 *
 **************************************************************************************************/
/// An opaque representation of a certain quantity, with the unit being encoded in the type.
/// This type implements checked addition and subtraction, and only permits type-safe multiplication.
#[derive(Serialize, Deserialize)]
pub struct GasQuantity<U> {
    val: u64,
    phantom: PhantomData<U>,
}

pub type InternalGas = GasQuantity<InternalGasUnit>;

pub type NumBytes = GasQuantity<Byte>;

pub type NumArgs = GasQuantity<Arg>;

/// An abstract measurement of the memory footprint of some Move concept (e.g. value, type etc.)
/// in the Move VM.
///
/// This is a legacy concept that is not well defined and will be deprecated very soon.
/// New applications should not be using this.
pub type AbstractMemorySize = GasQuantity<AbstractMemoryUnit>;

pub type InternalGasPerByte = GasQuantity<UnitDiv<InternalGasUnit, Byte>>;

pub type InternalGasPerAbstractMemoryUnit =
    GasQuantity<UnitDiv<InternalGasUnit, AbstractMemoryUnit>>;

pub type InternalGasPerArg = GasQuantity<UnitDiv<InternalGasUnit, Arg>>;

/// Abstract size of Rust Box pointer
/// This might be an over-estimation in many cases
pub const BOX_ABSTRACT_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);

/// Base abstract size of Rust enum
pub const ENUM_BASE_ABSTRACT_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/***************************************************************************************************
 * Constructors
 *
 **************************************************************************************************/
impl<U> GasQuantity<U> {
    pub const fn new(val: u64) -> Self {
        Self {
            val,
            phantom: PhantomData,
        }
    }

    pub const fn zero() -> Self {
        Self::new(0)
    }

    pub const fn one() -> Self {
        Self::new(1)
    }

    pub const fn is_zero(&self) -> bool {
        self.val == 0
    }
}

/***************************************************************************************************
 * Conversion
 *
 **************************************************************************************************/
impl<U> From<u64> for GasQuantity<U> {
    fn from(val: u64) -> Self {
        Self::new(val)
    }
}

// TODO(Gas): This allows the gas value to escape the monad, which weakens the type-level
//            protection it provides. It's currently needed for practical reasons but
//            we should look for ways to get rid of it.
impl<U> From<GasQuantity<U>> for u64 {
    fn from(gas: GasQuantity<U>) -> Self {
        gas.val
    }
}

/***************************************************************************************************
 * Clone & Copy
 *
 **************************************************************************************************/
impl<U> Clone for GasQuantity<U> {
    fn clone(&self) -> Self {
        Self::new(self.val)
    }
}

impl<U> Copy for GasQuantity<U> {}

/***************************************************************************************************
 * Display & Debug
 *
 **************************************************************************************************/
impl<U> Display for GasQuantity<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.val)
    }
}

impl<U> Debug for GasQuantity<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.val, std::any::type_name::<U>())
    }
}

/***************************************************************************************************
 * Comparison
 *
 **************************************************************************************************/
impl<U> GasQuantity<U> {
    fn cmp_impl(&self, other: &Self) -> Ordering {
        self.val.cmp(&other.val)
    }
}

impl<U> PartialEq for GasQuantity<U> {
    fn eq(&self, other: &Self) -> bool {
        matches!(self.cmp_impl(other), Ordering::Equal)
    }
}

impl<U> Eq for GasQuantity<U> {}

impl<U> PartialOrd for GasQuantity<U> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp_impl(other))
    }
}

impl<U> Ord for GasQuantity<U> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_impl(other)
    }
}

/***************************************************************************************************
 * Addition & Subtraction
 *
 **************************************************************************************************/
impl<U> Add<GasQuantity<U>> for GasQuantity<U> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.val.saturating_add(rhs.val))
    }
}

impl<U> AddAssign<GasQuantity<U>> for GasQuantity<U> {
    fn add_assign(&mut self, rhs: GasQuantity<U>) {
        *self = *self + rhs
    }
}

impl<U> GasQuantity<U> {
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.val.checked_sub(other.val).map(Self::new)
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        self.val.saturating_sub(other.val).into()
    }
}

/***************************************************************************************************
 * Multiplication
 *
 **************************************************************************************************/
fn mul_impl<U1, U2>(x: GasQuantity<U2>, y: GasQuantity<UnitDiv<U1, U2>>) -> GasQuantity<U1> {
    GasQuantity::new(x.val.saturating_mul(y.val))
}

impl<U1, U2> Mul<GasQuantity<UnitDiv<U1, U2>>> for GasQuantity<U2> {
    type Output = GasQuantity<U1>;

    fn mul(self, rhs: GasQuantity<UnitDiv<U1, U2>>) -> Self::Output {
        mul_impl(self, rhs)
    }
}

impl<U1, U2> Mul<GasQuantity<U2>> for GasQuantity<UnitDiv<U1, U2>> {
    type Output = GasQuantity<U1>;

    fn mul(self, rhs: GasQuantity<U2>) -> Self::Output {
        mul_impl(rhs, self)
    }
}

/***************************************************************************************************
 * To Unit
 *
 **************************************************************************************************/
fn apply_ratio_round_down(val: u64, nominator: u64, denominator: u64) -> u64 {
    assert_ne!(nominator, 0);
    assert_ne!(denominator, 0);

    let res = val as u128 * nominator as u128 / denominator as u128;
    if res > u64::MAX as u128 {
        u64::MAX
    } else {
        res as u64
    }
}

fn apply_ratio_round_up(val: u64, nominator: u64, denominator: u64) -> u64 {
    assert_ne!(nominator, 0);
    assert_ne!(denominator, 0);

    let n = val as u128 * nominator as u128;
    let d = denominator as u128;

    let res = n / d + if n % d == 0 { 0 } else { 1 };
    if res > u64::MAX as u128 {
        u64::MAX
    } else {
        res as u64
    }
}

/// Trait that defines a conversion from one unit to another, with a statically-determined
/// integral conversion rate.
pub trait ToUnit<U> {
    const MULTIPLIER: u64;
}

/// Trait that defines a conversion from one unit to another, with a statically-determined
/// fractional conversion rate.
pub trait ToUnitFractional<U> {
    const NOMINATOR: u64;
    const DENOMINATOR: u64;
}

impl<U> GasQuantity<U> {
    /// Convert the quantity to another unit.
    /// An integral multiplier must have been defined via the `ToUnit` trait.
    pub fn to_unit<T>(self) -> GasQuantity<T>
    where
        U: ToUnit<T>,
    {
        assert_ne!(U::MULTIPLIER, 0);

        GasQuantity::new(self.val.saturating_mul(U::MULTIPLIER))
    }

    /// Convert the quantity to another unit, with the resulting scalar value being rounded down.
    /// A ratio must have been defined via the `ToUnitFractional` trait.
    pub fn to_unit_round_down<T>(self) -> GasQuantity<T>
    where
        U: ToUnitFractional<T>,
    {
        GasQuantity::new(apply_ratio_round_down(
            self.val,
            U::NOMINATOR,
            U::DENOMINATOR,
        ))
    }

    /// Convert the quantity to another unit, with the resulting scalar value being rounded up.
    /// A ratio must have been defined via the `ToUnitFractional` trait.
    pub fn to_unit_round_up<T>(self) -> GasQuantity<T>
    where
        U: ToUnitFractional<T>,
    {
        GasQuantity::new(apply_ratio_round_up(self.val, U::NOMINATOR, U::DENOMINATOR))
    }
}

impl ToUnit<Byte> for KibiByte {
    const MULTIPLIER: u64 = 1024;
}

impl ToUnit<Byte> for MebiByte {
    const MULTIPLIER: u64 = 1024 * 1024;
}

impl ToUnit<Byte> for GibiByte {
    const MULTIPLIER: u64 = 1024 * 1024 * 1024;
}

impl ToUnit<KibiByte> for MebiByte {
    const MULTIPLIER: u64 = 1024;
}

impl ToUnit<KibiByte> for GibiByte {
    const MULTIPLIER: u64 = 1024 * 1024;
}

impl ToUnit<MebiByte> for GibiByte {
    const MULTIPLIER: u64 = 1024;
}

impl ToUnitFractional<KibiByte> for Byte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024;
}

impl ToUnitFractional<MebiByte> for KibiByte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024;
}

impl ToUnitFractional<MebiByte> for Byte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024 * 1024;
}

impl ToUnitFractional<GibiByte> for MebiByte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024;
}

impl ToUnitFractional<GibiByte> for KibiByte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024 * 1024;
}

impl ToUnitFractional<GibiByte> for Byte {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1024 * 1024 * 1024;
}

/***************************************************************************************************
 * To Unit With Params
 *
 **************************************************************************************************/
/// Trait that defines a conversion from one unit to another, with an integral conversion rate
/// determined from the parameters dynamically.
pub trait ToUnitWithParams<U> {
    type Params;

    fn multiplier(params: &Self::Params) -> u64;
}

/// Trait that defines a conversion from one unit to another, with a fractional conversion rate
/// determined from the parameters dynamically.
pub trait ToUnitFractionalWithParams<U> {
    type Params;

    fn ratio(params: &Self::Params) -> (u64, u64);
}

impl<U> GasQuantity<U> {
    /// Convert the quantity to another unit.
    /// An integral multiplier must have been defined via the `ToUnitWithParams` trait.
    pub fn to_unit_with_params<T>(
        self,
        params: &<U as ToUnitWithParams<T>>::Params,
    ) -> GasQuantity<T>
    where
        U: ToUnitWithParams<T>,
    {
        let multiplier = <U as ToUnitWithParams<T>>::multiplier(params);
        assert_ne!(multiplier, 0);
        GasQuantity::new(self.val.saturating_mul(multiplier))
    }

    /// Convert the quantity to another unit, with the resulting scalar value being rounded down.
    /// A ratio must have been defined via the `ToUnitFractionalWithParams` trait.
    pub fn to_unit_round_down_with_params<T>(
        self,
        params: &<U as ToUnitFractionalWithParams<T>>::Params,
    ) -> GasQuantity<T>
    where
        U: ToUnitFractionalWithParams<T>,
    {
        let (n, d) = <U as ToUnitFractionalWithParams<T>>::ratio(params);
        GasQuantity::new(apply_ratio_round_down(self.val, n, d))
    }

    /// Convert the quantity to another unit, with the resulting scalar value being rounded up.
    /// A ratio must have been defined via the `ToUnitFractionalWithParams` trait.
    pub fn to_unit_round_up_with_params<T>(
        self,
        params: &<U as ToUnitFractionalWithParams<T>>::Params,
    ) -> GasQuantity<T>
    where
        U: ToUnitFractionalWithParams<T>,
    {
        let (n, d) = <U as ToUnitFractionalWithParams<T>>::ratio(params);
        GasQuantity::new(apply_ratio_round_up(self.val, n, d))
    }
}
