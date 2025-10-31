use std::{fmt::Debug, fmt::Display, path::PathBuf};

use move_core_types::{
    account_address::{AccountAddress, AccountAddressParseError},
    identifier::Identifier,
};
use serde::{Deserialize, Serialize};

use crate::errors::fmt_truncated;

use super::EnvironmentID;

// TODO(Manos): Let's use a less free name...
pub type EnvironmentName = String;

pub type PackageName = Identifier;

// TODO: this doesn't really belong in `schema` (or at least it should follow the format of other
// schema data structures of being a plain old object)
#[derive(Debug, Clone)]
pub struct Environment {
    pub name: EnvironmentName,
    pub id: EnvironmentID,
}

impl Environment {
    pub fn new(name: EnvironmentName, id: EnvironmentID) -> Self {
        Self { name, id }
    }

    pub fn name(&self) -> &EnvironmentName {
        &self.name
    }

    pub fn id(&self) -> &EnvironmentID {
        &self.id
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublishedID(pub AccountAddress);

#[derive(Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OriginalID(pub AccountAddress);

/// A pair of published-at and original-id; appears in various places
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct PublishAddresses {
    pub published_at: PublishedID,
    pub original_id: OriginalID,
}

/// A serialized dependency of the form `{ local = <path> }`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDepInfo {
    /// The path on the filesystem, relative to the location of the containing file
    pub local: PathBuf,
}

/// An on-chain dependency `{on-chain = true}`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct OnChainDepInfo {
    #[serde(rename = "on-chain")]
    pub on_chain: ConstTrue,
}

/// The constant `true`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "bool", into = "bool")]
pub struct ConstTrue;

impl PublishAddresses {
    pub fn zero() -> Self {
        Self {
            published_at: PublishedID(AccountAddress::ZERO),
            original_id: OriginalID(AccountAddress::ZERO),
        }
    }
}

impl From<u16> for OriginalID {
    fn from(value: u16) -> Self {
        Self(AccountAddress::from_suffix(value))
    }
}

impl From<u16> for PublishedID {
    fn from(value: u16) -> Self {
        Self(AccountAddress::from_suffix(value))
    }
}

impl TryFrom<&str> for PublishedID {
    type Error = AccountAddressParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(AccountAddress::from_hex(value)?))
    }
}

impl TryFrom<&str> for OriginalID {
    type Error = AccountAddressParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(AccountAddress::from_hex(value)?))
    }
}

impl TryFrom<bool> for ConstTrue {
    type Error = &'static str;

    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if !value {
            return Err("Expected the constant `true`");
        }
        Ok(Self)
    }
}

impl From<ConstTrue> for bool {
    fn from(_: ConstTrue) -> Self {
        true
    }
}

/// Serialize an `AccountAddress` to its canonical string representation
fn ser_account<S>(account: &AccountAddress, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let canonical_string = account.to_canonical_string(true);
    serializer.serialize_str(&canonical_string)
}

impl Serialize for OriginalID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ser_account(&self.0, serializer)
    }
}

impl Serialize for PublishedID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ser_account(&self.0, serializer)
    }
}

impl PublishedID {
    pub fn truncated(&self) -> String {
        fmt_truncated(self.0.to_canonical_string(true), 4, 4)
    }
}

impl OriginalID {
    pub fn truncated(&self) -> String {
        fmt_truncated(self.0.to_canonical_string(true), 4, 4)
    }
}

impl Display for PublishedID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_canonical_string(true))
    }
}

impl Debug for PublishedID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_canonical_string(true))
    }
}

impl Display for OriginalID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_canonical_string(true))
    }
}

impl Debug for OriginalID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_canonical_string(true))
    }
}
