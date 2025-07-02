use std::{fmt::Debug, fmt::Display, path::PathBuf};

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

pub type EnvironmentName = String;
pub type PackageName = Identifier;

#[derive(Clone, Deserialize)]
pub struct PublishedID(pub AccountAddress);

#[derive(Clone, Deserialize)]
pub struct OriginalID(pub AccountAddress);

/// A serialized dependency of the form `{ local = <path> }`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LocalDepInfo {
    /// The path on the filesystem, relative to the location of the containing file
    pub local: PathBuf,
}

/// An on-chain dependency `{on-chain = true}`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OnChainDepInfo {
    #[serde(rename = "on-chain")]
    on_chain: ConstTrue,
}

/// The constant `true`
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(try_from = "bool", into = "bool")]
struct ConstTrue;

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
    fn from(value: ConstTrue) -> Self {
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
