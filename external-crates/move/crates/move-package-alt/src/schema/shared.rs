use std::path::PathBuf;

use move_core_types::identifier::Identifier;
use serde::{Deserialize, Serialize};

pub type EnvironmentName = String;
pub type PackageName = Identifier;

// TODO: this should be an OID
pub type Address = String;

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
