// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::ScalarType;
use move_core_types::language_storage::StructTag;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sui_name_service::{validate_label, Domain};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    collection_types::VecMap,
    dynamic_field::Field,
    id::ID,
    object::MoveObject as NativeMoveObject,
};

use crate::{
    config::{MoveRegistryConfig, MOVE_REGISTRY_MODULE, MOVE_REGISTRY_TYPE},
    types::base64::Base64,
};

use super::error::MoveRegistryError;

/// Regex to parse a dot move name. Version is optional (defaults to latest).
/// For versioned format, the expected format is `@org/app/1` (1 == version).
/// For an unversioned format, the expected format is `@org/app`.
///
/// The unbound regex can be used to search matches in a type tag.
/// Use `VERSIONED_NAME_REGEX` for parsing a single name from a str.
const VERSIONED_NAME_UNBOUND_REGEX: &str =
    concat!(r"([a-z0-9.\-@]*)", r"\/", "([a-z0-9.-]*)", r"(?:\/(\d+))?",);

/// Regex to parse a dot move name. Version is optional (defaults to latest).
/// For versioned format, the expected format is `@org/app/1`.
/// For an unversioned format, the expected format is `@org/app`.
///
/// This regex is used to parse a single name (does not do type_tag matching).
/// Use `VERSIONED_NAME_UNBOUND_REGEX` for type tag matching.
const VERSIONED_NAME_REGEX: &str = concat!(
    "^",
    r"([a-z0-9.\-@]*)",
    r"\/",
    "([a-z0-9.-]*)",
    r"(?:\/(\d+))?",
    "$"
);

/// A regular expression that detects all possible dot move names in a type tag.
pub(crate) static VERSIONED_NAME_UNBOUND_REG: Lazy<Regex> =
    Lazy::new(|| Regex::new(VERSIONED_NAME_UNBOUND_REGEX).unwrap());

/// A regular expression that detects a single name in the format `@org/app/1`.
pub(crate) static VERSIONED_NAME_REG: Lazy<Regex> =
    Lazy::new(|| Regex::new(VERSIONED_NAME_REGEX).unwrap());

/// An AppRecord entry in the DotMove service.
/// Attention: The format of this struct should not change unless the on-chain format changes,
/// as we define it to deserialize on-chain data.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub(crate) struct AppRecord {
    pub(crate) app_cap_id: ID,
    pub(crate) ns_nft_id: ID,
    pub(crate) app_info: Option<AppInfo>,
    pub(crate) networks: VecMap<String, AppInfo>,
    pub(crate) metadata: VecMap<String, String>,
    pub(crate) storage: ObjectID,
}

/// Attention: The format of this struct should not change unless the on-chain format changes,
/// as we define it to deserialize on-chain data.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub(crate) struct AppInfo {
    pub(crate) package_info_id: Option<ID>,
    pub(crate) package_address: Option<SuiAddress>,
    pub(crate) upgrade_cap_id: Option<ID>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub(crate) struct VersionedName {
    /// A version name defaults at None, which means we need the latest version.
    pub(crate) version: Option<u64>,
    /// The on-chain `Name` object that represents the move registry name.
    pub(crate) name: Name,
}

/// Attention: The format of this struct should not change unless the on-chain format changes,
/// as we define it to deserialize on-chain data.
#[derive(Debug, Serialize, Deserialize, Hash, Clone, Eq, PartialEq)]
pub(crate) struct Name {
    pub(crate) org: Domain,
    pub(crate) app: Vec<String>,
}

impl Name {
    pub(crate) fn new(org: Domain, app: &str) -> Self {
        Self {
            org,
            app: vec![app.to_string()],
        }
    }

    pub(crate) fn type_(package_address: SuiAddress) -> StructTag {
        StructTag {
            address: package_address.into(),
            module: MOVE_REGISTRY_MODULE.to_owned(),
            name: MOVE_REGISTRY_TYPE.to_owned(),
            type_params: vec![],
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        bcs::to_bytes(&self).unwrap()
    }

    pub(crate) fn to_base64_string(&self) -> String {
        Base64::from(self.to_bytes()).to_value().to_string()
    }

    /// Generate the ObjectID for a given `Name`
    pub(crate) fn to_dynamic_field_id(
        &self,
        config: &MoveRegistryConfig,
    ) -> Result<ObjectID, bcs::Error> {
        let domain_type_tag = Self::type_(config.package_address);

        sui_types::dynamic_field::derive_dynamic_field_id(
            config.registry_id,
            &sui_types::TypeTag::Struct(Box::new(domain_type_tag)),
            &self.to_bytes(),
        )
    }
}

impl FromStr for VersionedName {
    type Err = MoveRegistryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some(caps) = VERSIONED_NAME_REG.captures(s) else {
            return Err(MoveRegistryError::InvalidName(s.to_string()));
        };

        let Some(org_name) = caps.get(1).map(|x| x.as_str()) else {
            return Err(MoveRegistryError::InvalidName(s.to_string()));
        };

        // validate org_name by trying to cast our input to a Domain.
        let domain = Domain::from_str(org_name)
            .map_err(|_| MoveRegistryError::InvalidName(s.to_string()))?;

        let Some(app_name) = caps.get(2).map(|x| x.as_str()) else {
            return Err(MoveRegistryError::InvalidName(s.to_string()));
        };

        // Validate our app's label.
        validate_label(app_name).map_err(|_| MoveRegistryError::InvalidName(s.to_string()))?;

        let version: Option<u64> = caps
            .get(3)
            .map(|x| x.as_str().parse())
            .transpose()
            .map_err(|_| MoveRegistryError::InvalidVersion)?;

        Ok(Self {
            version,
            name: Name::new(domain, app_name),
        })
    }
}

impl TryFrom<NativeMoveObject> for AppRecord {
    type Error = MoveRegistryError;

    fn try_from(object: NativeMoveObject) -> Result<Self, MoveRegistryError> {
        object
            .to_rust::<Field<Name, Self>>()
            .map(|record| record.value)
            .ok_or_else(|| MoveRegistryError::FailedToDeserializeRecord(object.id()))
    }
}

#[cfg(test)]
mod tests {
    use super::VersionedName;
    use std::str::FromStr;

    #[test]
    fn parse_some_names() {
        let versioned = VersionedName::from_str("@org/app/1").unwrap();
        assert!(versioned.version.is_some_and(|x| x == 1));

        assert!(VersionedName::from_str("@org/app/34")
            .unwrap()
            .version
            .is_some_and(|x| x == 34));
        assert!(VersionedName::from_str("@org/app")
            .unwrap()
            .version
            .is_none());

        let ok_names = vec![
            "@org/1-app/1",
            "@org/1-app/34",
            "@org/1-app",
            "nested@org/app",
            "even.more.nested@org/app",
        ];

        let composite_ok_names = vec![
            format!("@org/{}/1", generate_fixed_string(63)),
            format!("@org/{}-app/34", generate_fixed_string(59)),
            format!(
                "@{}/{}",
                generate_fixed_string(63),
                generate_fixed_string(63)
            ),
            format!(
                "@{}/{}-{}",
                generate_fixed_string(63),
                generate_fixed_string(30),
                generate_fixed_string(30)
            ),
        ];

        for name in ok_names {
            assert!(VersionedName::from_str(name).is_ok());
        }
        for name in composite_ok_names {
            assert!(VersionedName::from_str(&name).is_ok());
        }

        let not_ok_names = vec![
            "@org/-app",
            "@org/1.app",
            "@org/app-",
            "@org/app--",
            "@org/app?",
            "@org/app/",
            "@org/app/v",
            "@org/app/veh",
            "@org",
            "@/veh/app",
            "app",
            "@",
            "@org/ap@",
            "!org/ap",
            "#org/ap",
            "@org/ap#org",
            "org/@app",
            "",
            " ",
        ];
        let composite_err_names = vec![
            format!(
                "@{}{}--/{}",
                generate_fixed_string(10),
                generate_fixed_string(10),
                generate_fixed_string(63)
            ),
            format!(
                "@--{}-{}/{}",
                generate_fixed_string(10),
                generate_fixed_string(10),
                generate_fixed_string(63)
            ),
            format!(
                "@{}/--{}{}",
                generate_fixed_string(63),
                generate_fixed_string(30),
                generate_fixed_string(30)
            ),
        ];

        for name in not_ok_names {
            assert!(VersionedName::from_str(name).is_err());
        }
        for name in composite_err_names {
            assert!(VersionedName::from_str(&name).is_err());
        }
    }

    fn generate_fixed_string(len: usize) -> String {
        // Define the characters to use in the string
        let chars = "abcdefghijklmnopqrstuvwxyz0123456789";
        // Repeat the characters to ensure we have at least 63 characters
        let repeated_chars = chars.repeat(3);

        // Take the first 63 characters to form the string
        let fixed_string = &repeated_chars[0..len];

        fixed_string.to_string()
    }
}
