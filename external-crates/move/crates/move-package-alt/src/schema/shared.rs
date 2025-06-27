use std::path::PathBuf;

use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde::{Deserialize, Serialize};

pub type EnvironmentName = String;
pub type PackageName = Identifier;

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
