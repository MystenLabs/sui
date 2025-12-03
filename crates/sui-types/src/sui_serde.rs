// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fmt::Write;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::ops::Deref;
use std::str::FromStr;

use move_core_types::account_address::AccountAddress;
use move_core_types::language_storage::{StructTag, TypeTag};
use schemars::JsonSchema;
use serde;
use serde::de::{Deserializer, Error};
use serde::ser::{Error as SerError, Serializer};
use serde::{Deserialize, Serialize};
use serde_with::DisplayFromStr;
use serde_with::serde_as;
use serde_with::{Bytes, DeserializeAs, SerializeAs};

use sui_protocol_config::ProtocolVersion;

use crate::{
    DEEPBOOK_ADDRESS, SUI_CLOCK_ADDRESS, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_ADDRESS,
    SUI_SYSTEM_STATE_ADDRESS, parse_sui_struct_tag, parse_sui_type_tag,
};

#[inline]
pub(crate) fn to_custom_deser_error<'de, D, E>(e: E) -> D::Error
where
    E: Debug,
    D: Deserializer<'de>,
{
    Error::custom(format!("byte deserialization failed, cause by: {:?}", e))
}

#[inline]
pub(crate) fn to_custom_ser_error<S, E>(e: E) -> S::Error
where
    E: Debug,
    S: Serializer,
{
    S::Error::custom(format!("byte serialization failed, cause by: {:?}", e))
}

/// Use with serde_as to control serde for human-readable serialization and deserialization
/// `H` : serde_as SerializeAs/DeserializeAs delegation for human readable in/output
/// `R` : serde_as SerializeAs/DeserializeAs delegation for non-human readable in/output
///
/// # Example:
///
/// ```text
/// #[serde_as]
/// #[derive(Deserialize, Serialize)]
/// struct Example(#[serde_as(as = "Readable<DisplayFromStr, _>")] [u8; 20]);
/// ```
///
/// The above example will delegate human-readable serde to `DisplayFromStr`
/// and array tuple (default) for non-human-readable serializer.
pub struct Readable<H, R> {
    human_readable: PhantomData<H>,
    non_human_readable: PhantomData<R>,
}

impl<T: ?Sized, H, R> SerializeAs<T> for Readable<H, R>
where
    H: SerializeAs<T>,
    R: SerializeAs<T>,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            H::serialize_as(value, serializer)
        } else {
            R::serialize_as(value, serializer)
        }
    }
}

impl<'de, R, H, T> DeserializeAs<'de, T> for Readable<H, R>
where
    H: DeserializeAs<'de, T>,
    R: DeserializeAs<'de, T>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            H::deserialize_as(deserializer)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}

pub struct SuiStructTag;

impl SerializeAs<StructTag> for SuiStructTag {
    fn serialize_as<S>(value: &StructTag, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let f = to_sui_struct_tag_string(value).map_err(S::Error::custom)?;
        f.serialize(serializer)
    }
}

const SUI_ADDRESSES: [AccountAddress; 7] = [
    AccountAddress::ZERO,
    AccountAddress::ONE,
    SUI_FRAMEWORK_ADDRESS,
    SUI_SYSTEM_ADDRESS,
    DEEPBOOK_ADDRESS,
    SUI_SYSTEM_STATE_ADDRESS,
    SUI_CLOCK_ADDRESS,
];
/// Serialize StructTag as a string, retaining the leading zeros in the address.
pub fn to_sui_struct_tag_string(value: &StructTag) -> Result<String, fmt::Error> {
    let mut f = String::new();
    // trim leading zeros if address is in SUI_ADDRESSES
    let address = if SUI_ADDRESSES.contains(&value.address) {
        value.address.short_str_lossless()
    } else {
        value.address.to_canonical_string(/* with_prefix */ false)
    };

    write!(f, "0x{}::{}::{}", address, value.module, value.name)?;
    if let Some(first_ty) = value.type_params.first() {
        write!(f, "<")?;
        write!(f, "{}", to_sui_type_tag_string(first_ty)?)?;
        for ty in value.type_params.iter().skip(1) {
            write!(f, ", {}", to_sui_type_tag_string(ty)?)?;
        }
        write!(f, ">")?;
    }
    Ok(f)
}

fn to_sui_type_tag_string(value: &TypeTag) -> Result<String, fmt::Error> {
    match value {
        TypeTag::Vector(t) => Ok(format!("vector<{}>", to_sui_type_tag_string(t)?)),
        TypeTag::Struct(s) => to_sui_struct_tag_string(s),
        _ => Ok(value.to_string()),
    }
}

impl<'de> DeserializeAs<'de, StructTag> for SuiStructTag {
    fn deserialize_as<D>(deserializer: D) -> Result<StructTag, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_sui_struct_tag(&s).map_err(D::Error::custom)
    }
}

pub struct SuiTypeTag;

impl SerializeAs<TypeTag> for SuiTypeTag {
    fn serialize_as<S>(value: &TypeTag, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = to_sui_type_tag_string(value).map_err(S::Error::custom)?;
        s.serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, TypeTag> for SuiTypeTag {
    fn deserialize_as<D>(deserializer: D) -> Result<TypeTag, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_sui_type_tag(&s).map_err(D::Error::custom)
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy, JsonSchema)]
pub struct BigInt<T>(
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    T,
)
where
    T: Display + FromStr,
    <T as FromStr>::Err: Display;

impl<T> BigInt<T>
where
    T: Display + FromStr,
    <T as FromStr>::Err: Display,
{
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> SerializeAs<T> for BigInt<T>
where
    T: Display + FromStr + Copy,
    <T as FromStr>::Err: Display,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BigInt(*value).serialize(serializer)
    }
}

impl<'de, T> DeserializeAs<'de, T> for BigInt<T>
where
    T: Display + FromStr + Copy,
    <T as FromStr>::Err: Display,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(*BigInt::deserialize(deserializer)?)
    }
}

impl<T> From<T> for BigInt<T>
where
    T: Display + FromStr,
    <T as FromStr>::Err: Display,
{
    fn from(v: T) -> BigInt<T> {
        BigInt(v)
    }
}

impl<T> Deref for BigInt<T>
where
    T: Display + FromStr,
    <T as FromStr>::Err: Display,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Display for BigInt<T>
where
    T: Display + FromStr,
    <T as FromStr>::Err: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy, JsonSchema)]
pub struct SequenceNumber(#[schemars(with = "BigInt<u64>")] u64);

impl SerializeAs<crate::base_types::SequenceNumber> for SequenceNumber {
    fn serialize_as<S>(
        value: &crate::base_types::SequenceNumber,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = value.value().to_string();
        s.serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, crate::base_types::SequenceNumber> for SequenceNumber {
    fn deserialize_as<D>(deserializer: D) -> Result<crate::base_types::SequenceNumber, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b = BigInt::deserialize(deserializer)?;
        Ok(crate::base_types::SequenceNumber::from_u64(*b))
    }
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy, JsonSchema)]
#[serde(rename = "ProtocolVersion")]
pub struct AsProtocolVersion(#[schemars(with = "BigInt<u64>")] u64);

impl SerializeAs<ProtocolVersion> for AsProtocolVersion {
    fn serialize_as<S>(value: &ProtocolVersion, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = value.as_u64().to_string();
        s.serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, ProtocolVersion> for AsProtocolVersion {
    fn deserialize_as<D>(deserializer: D) -> Result<ProtocolVersion, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b = BigInt::<u64>::deserialize(deserializer)?;
        Ok(ProtocolVersion::from(*b))
    }
}

/// Serializes and deserializes a RoaringBitmap with its own on-disk standard.
/// <https://github.com/RoaringBitmap/RoaringFormatSpec>
pub(crate) struct SuiBitmap;

impl SerializeAs<roaring::RoaringBitmap> for SuiBitmap {
    fn serialize_as<S>(source: &roaring::RoaringBitmap, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = vec![];

        source
            .serialize_into(&mut bytes)
            .map_err(to_custom_ser_error::<S, _>)?;
        Bytes::serialize_as(&bytes, serializer)
    }
}

impl<'de> DeserializeAs<'de, roaring::RoaringBitmap> for SuiBitmap {
    fn deserialize_as<D>(deserializer: D) -> Result<roaring::RoaringBitmap, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Bytes::deserialize_as(deserializer)?;
        deserialize_sui_bitmap(&bytes).map_err(to_custom_deser_error::<'de, D, _>)
    }
}

// RoaringBitmap::deserialize_from() or iter() do not check for duplicates.
// So this function is needed to sanitize the bitmap to ensure unique entries.
pub(crate) fn deserialize_sui_bitmap(bytes: &[u8]) -> std::io::Result<roaring::RoaringBitmap> {
    let orig_bitmap = roaring::RoaringBitmap::deserialize_from(bytes)?;
    // Ensure there is no duplicated entries in the bitmap.
    let mut seen = std::collections::BTreeSet::new();
    let mut new_bitmap = roaring::RoaringBitmap::new();
    for v in orig_bitmap.iter() {
        if seen.insert(v) {
            new_bitmap.insert(v);
        }
    }
    Ok(new_bitmap)
}

#[cfg(test)]
mod test {
    use base64::Engine as _;

    use super::*;

    #[test]
    fn test_sui_bitmap_unique_deserialize() {
        let raw = "OjAAAAoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWAAAAFoAAABcAAAAXgAAAGAAAABiAAAAZAAAAGYAAABoAAAAagAAAAEAAQABAAEAAQABAAEAAQABAAEA";
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .unwrap();

        let bitmap = roaring::RoaringBitmap::deserialize_from(&bytes[..]).unwrap();
        assert_eq!(bitmap.len(), 10);
        let bitmap_values: Vec<u32> = bitmap.iter().collect();
        assert_eq!(bitmap_values, vec![1; 10]);

        let sui_bitmap = deserialize_sui_bitmap(&bytes[..]).unwrap();
        assert_eq!(sui_bitmap.len(), 1);
        let bitmap_values: Vec<u32> = sui_bitmap.iter().collect();
        assert_eq!(bitmap_values, vec![1]);
    }
}
