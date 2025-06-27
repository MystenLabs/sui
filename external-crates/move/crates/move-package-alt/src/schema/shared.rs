use std::{fmt::Debug, fmt::Display, path::PathBuf};

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

pub type EnvironmentName = String;
pub type PackageName = Identifier;

#[derive(Clone)]
pub struct PublishedID(pub AccountAddress);

#[derive(Clone)]
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

/// Serialize an `AccountAddress` as a hex literal string, which will have the 0x prefix.
pub fn ser_account<S>(account: &AccountAddress, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let canonical_string = account.to_hex_literal();
    serializer.serialize_str(&canonical_string)
}

pub fn ser_opt_account<S>(
    account: &Option<AccountAddress>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(account) = account {
        ser_account(account, serializer)
    } else {
        serializer.serialize_none()
    }
}

impl Serialize for OriginalID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ser_account(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for OriginalID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let account = AccountAddress::deserialize(deserializer)?;
        Ok(OriginalID(account))
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

impl<'de> Deserialize<'de> for PublishedID {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let account = AccountAddress::deserialize(deserializer)?;
        Ok(PublishedID(account))
    }
}

impl Display for PublishedID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_hex_literal())
    }
}

impl Debug for PublishedID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_hex_literal())
    }
}

impl Display for OriginalID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_hex_literal())
    }
}

impl Debug for OriginalID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_hex_literal())
    }
}
