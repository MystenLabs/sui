// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::collection_types::VecMap;
use sui_types::dynamic_field::Field;
use sui_types::id::{ID, UID};
use sui_types::object::{MoveObject, Object};
use sui_types::TypeTag;

const NAME_SERVICE_DOMAIN_MODULE: &IdentStr = ident_str!("domain");
const NAME_SERVICE_DOMAIN_STRUCT: &IdentStr = ident_str!("Domain");
const LEAF_EXPIRATION_TIMESTAMP: u64 = 0;
const DEFAULT_TLD: &str = "sui";
const ACCEPTED_SEPARATORS: [char; 2] = ['.', '*'];
const SUI_NEW_FORMAT_SEPARATOR: char = '@';

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Registry {
    /// The `registry` table maps `Domain` to `NameRecord`.
    /// Added / replaced in the `add_record` function.
    registry: Table<Domain, NameRecord>,
    /// The `reverse_registry` table maps `address` to `domain_name`.
    /// Updated in the `set_reverse_lookup` function.
    reverse_registry: Table<SuiAddress, Domain>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, Hash, PartialEq)]
pub struct Domain {
    labels: Vec<String>,
}

/// A single record in the registry.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct NameRecord {
    /// The ID of the `RegistrationNFT` assigned to this record.
    ///
    /// The owner of the corrisponding `RegistrationNFT` has the rights to
    /// be able to change and adjust the `target_address` of this domain.
    ///
    /// It is possible that the ID changes if the record expires and is
    /// purchased by someone else.
    pub nft_id: ID,
    /// Timestamp in milliseconds when the record expires.
    pub expiration_timestamp_ms: u64,
    /// The target address that this domain points to
    pub target_address: Option<SuiAddress>,
    /// Additional data which may be stored in a record
    pub data: VecMap<String, String>,
}

/// A SuinsRegistration object to manage an SLD
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SuinsRegistration {
    pub id: UID,
    pub domain: Domain,
    pub domain_name: String,
    pub expiration_timestamp_ms: u64,
    pub image_url: String,
}

/// A SubDomainRegistration object to manage a subdomain.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SubDomainRegistration {
    pub id: UID,
    pub nft: SuinsRegistration,
}

/// Two different view options for a domain.
/// `At` -> `test@example` | `Dot` -> `test.example.sui`
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum DomainFormat {
    At,
    Dot,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct NameServiceConfig {
    pub package_address: SuiAddress,
    pub registry_id: ObjectID,
    pub reverse_registry_id: ObjectID,
}

/// Rust version of the Move sui::table::Table type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Table<K, V> {
    pub id: ObjectID,
    pub size: u64,

    #[serde(skip)]
    _key: PhantomData<K>,
    #[serde(skip)]
    _value: PhantomData<V>,
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum NameServiceError {
    #[error("Name Service: String length: {0} exceeds maximum allowed length: {1}")]
    ExceedsMaxLength(usize, usize),
    #[error("Name Service: String length: {0} outside of valid range: [{1}, {2}]")]
    InvalidLength(usize, usize, usize),
    #[error("Name Service: Hyphens are not allowed as the first or last character")]
    InvalidHyphens,
    #[error("Name Service: Only lowercase letters, numbers, and hyphens are allowed")]
    InvalidUnderscore,
    #[error("Name Service: Domain must contain at least one label")]
    LabelsEmpty,
    #[error("Name Service: Domain must include only one separator")]
    InvalidSeparator,

    #[error("Name Service: Name has expired.")]
    NameExpired,
    #[error("Name Service: Malformed object for {0}")]
    MalformedObject(ObjectID),
}

impl Domain {
    pub fn type_(package_address: SuiAddress) -> StructTag {
        StructTag {
            address: package_address.into(),
            module: NAME_SERVICE_DOMAIN_MODULE.to_owned(),
            name: NAME_SERVICE_DOMAIN_STRUCT.to_owned(),
            type_params: vec![],
        }
    }

    /// Derive the parent domain for a given domain
    /// E.g. `test.example.sui` -> `example.sui`
    ///
    /// SAFETY: This is a safe operation because we only allow a
    /// domain's label vector size to be >= 2 (see `Domain::from_str`)
    pub fn parent(&self) -> Domain {
        Domain {
            labels: self.labels[0..(self.labels.len() - 1)].to_vec(),
        }
    }

    pub fn is_subdomain(&self) -> bool {
        self.depth() >= 3
    }

    /// Returns the depth for a name.
    /// Depth is defined by the amount of labels in a domain, including TLD.
    /// E.g. `test.example.sui` -> `3`
    ///
    /// SAFETY: We can safely cast to a u8 as the max depth is 235.
    pub fn depth(&self) -> u8 {
        self.labels.len() as u8
    }

    /// Formats a domain into a string based on the available output formats.
    /// The default separator is `.`
    pub fn format(&self, format: DomainFormat) -> String {
        let mut labels = self.labels.clone();
        let sep = &ACCEPTED_SEPARATORS[0].to_string();
        labels.reverse();

        if format == DomainFormat::Dot {
            return labels.join(sep);
        };

        // SAFETY: This is a safe operation because we only allow a
        // domain's label vector size to be >= 2 (see `Domain::from_str`)
        let _tld = labels.pop();
        let sld = labels.pop().unwrap();

        format!("{}{}{}", labels.join(sep), SUI_NEW_FORMAT_SEPARATOR, sld)
    }
}

impl NameServiceConfig {
    pub fn new(
        package_address: SuiAddress,
        registry_id: ObjectID,
        reverse_registry_id: ObjectID,
    ) -> Self {
        Self {
            package_address,
            registry_id,
            reverse_registry_id,
        }
    }

    pub fn record_field_id(&self, domain: &Domain) -> ObjectID {
        let domain_type_tag = Domain::type_(self.package_address);
        let domain_bytes = bcs::to_bytes(domain).unwrap();

        sui_types::dynamic_field::derive_dynamic_field_id(
            self.registry_id,
            &TypeTag::Struct(Box::new(domain_type_tag)),
            &domain_bytes,
        )
        .unwrap()
    }

    pub fn reverse_record_field_id(&self, address: &[u8]) -> ObjectID {
        sui_types::dynamic_field::derive_dynamic_field_id(
            self.reverse_registry_id,
            &TypeTag::Address,
            address,
        )
        .unwrap()
    }

    // Create a config based on the package and object ids published on mainnet
    pub fn mainnet() -> Self {
        const MAINNET_NS_PACKAGE_ADDRESS: &str =
            "0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0";
        const MAINNET_NS_REGISTRY_ID: &str =
            "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106";
        const MAINNET_NS_REVERSE_REGISTRY_ID: &str =
            "0x2fd099e17a292d2bc541df474f9fafa595653848cbabb2d7a4656ec786a1969f";

        let package_address = SuiAddress::from_str(MAINNET_NS_PACKAGE_ADDRESS).unwrap();
        let registry_id = ObjectID::from_str(MAINNET_NS_REGISTRY_ID).unwrap();
        let reverse_registry_id = ObjectID::from_str(MAINNET_NS_REVERSE_REGISTRY_ID).unwrap();

        Self::new(package_address, registry_id, reverse_registry_id)
    }

    // Create a config based on the package and object ids published on testnet
    pub fn testnet() -> Self {
        const TESTNET_NS_PACKAGE_ADDRESS: &str =
            "0x22fa05f21b1ad71442491220bb9338f7b7095fe35000ef88d5400d28523bdd93";
        const TESTNET_NS_REGISTRY_ID: &str =
            "0xb120c0d55432630fce61f7854795a3463deb6e3b443cc4ae72e1282073ff56e4";
        const TESTNET_NS_REVERSE_REGISTRY_ID: &str =
            "0xcee9dbb070db70936c3a374439a6adb16f3ba97eac5468d2e1e6fff6ed93e465";

        let package_address = SuiAddress::from_str(TESTNET_NS_PACKAGE_ADDRESS).unwrap();
        let registry_id = ObjectID::from_str(TESTNET_NS_REGISTRY_ID).unwrap();
        let reverse_registry_id = ObjectID::from_str(TESTNET_NS_REVERSE_REGISTRY_ID).unwrap();

        Self::new(package_address, registry_id, reverse_registry_id)
    }
}

impl NameRecord {
    /// Leaf records expire when their parent expires.
    /// The `expiration_timestamp_ms` is set to `0` (on-chain) to indicate this.
    pub fn is_leaf_record(&self) -> bool {
        self.expiration_timestamp_ms == LEAF_EXPIRATION_TIMESTAMP
    }

    /// Validate that a `NameRecord` is a valid parent of a child `NameRecord`.
    ///
    /// WARNING: This only applies for `leaf` records
    pub fn is_valid_leaf_parent(&self, child: &NameRecord) -> bool {
        self.nft_id == child.nft_id
    }

    /// Checks if a `node` name record has expired.
    /// Expects the latest checkpoint's timestamp.
    pub fn is_node_expired(&self, checkpoint_timestamp_ms: u64) -> bool {
        self.expiration_timestamp_ms < checkpoint_timestamp_ms
    }
}

impl FromStr for Domain {
    type Err = NameServiceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// The maximum length of a full domain
        const MAX_DOMAIN_LENGTH: usize = 200;

        if s.len() > MAX_DOMAIN_LENGTH {
            return Err(NameServiceError::ExceedsMaxLength(
                s.len(),
                MAX_DOMAIN_LENGTH,
            ));
        }
        let separator = separator(s)?;

        let formatted_string = convert_from_new_format(s, &separator)?;

        let labels = formatted_string
            .split(separator)
            .rev()
            .map(validate_label)
            .collect::<Result<Vec<_>, Self::Err>>()?;

        // A valid domain in our system has at least a TLD and an SLD (len == 2).
        if labels.len() < 2 {
            return Err(NameServiceError::LabelsEmpty);
        }

        let labels = labels.into_iter().map(ToOwned::to_owned).collect();
        Ok(Domain { labels })
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We use to_string() to check on-chain state and parse on-chain data
        // so we should always default to DOT format.
        let output = self.format(DomainFormat::Dot);
        f.write_str(&output)?;

        Ok(())
    }
}

impl Default for NameServiceConfig {
    fn default() -> Self {
        Self::mainnet()
    }
}

impl TryFrom<Object> for NameRecord {
    type Error = NameServiceError;

    fn try_from(object: Object) -> Result<Self, NameServiceError> {
        object
            .to_rust::<Field<Domain, Self>>()
            .map(|record| record.value)
            .ok_or_else(|| NameServiceError::MalformedObject(object.id()))
    }
}

impl TryFrom<MoveObject> for NameRecord {
    type Error = NameServiceError;

    fn try_from(object: MoveObject) -> Result<Self, NameServiceError> {
        object
            .to_rust::<Field<Domain, Self>>()
            .map(|record| record.value)
            .ok_or_else(|| NameServiceError::MalformedObject(object.id()))
    }
}

/// Parses a separator from the domain string input.
/// E.g.  `example.sui` -> `.` | example*sui -> `@` | `example*sui` -> `*`
fn separator(s: &str) -> Result<char, NameServiceError> {
    let mut domain_separator: Option<char> = None;

    for separator in ACCEPTED_SEPARATORS.iter() {
        if s.contains(*separator) {
            if domain_separator.is_some() {
                return Err(NameServiceError::InvalidSeparator);
            }

            domain_separator = Some(*separator);
        }
    }

    match domain_separator {
        Some(separator) => Ok(separator),
        None => Ok(ACCEPTED_SEPARATORS[0]),
    }
}

/// Converts @label ending to label{separator}sui ending.
///
/// E.g. `@example` -> `example.sui` | `test@example` -> `test.example.sui`
fn convert_from_new_format(s: &str, separator: &char) -> Result<String, NameServiceError> {
    let mut splits = s.split(SUI_NEW_FORMAT_SEPARATOR);

    let Some(before) = splits.next() else {
        return Err(NameServiceError::InvalidSeparator);
    };

    let Some(after) = splits.next() else {
        return Ok(before.to_string());
    };

    if splits.next().is_some() || after.contains(*separator) || after.is_empty() {
        return Err(NameServiceError::InvalidSeparator);
    }

    let mut parts = vec![];

    if !before.is_empty() {
        parts.push(before);
    }

    parts.push(after);
    parts.push(DEFAULT_TLD);

    Ok(parts.join(&separator.to_string()))
}

pub fn validate_label(label: &str) -> Result<&str, NameServiceError> {
    const MIN_LABEL_LENGTH: usize = 1;
    const MAX_LABEL_LENGTH: usize = 63;
    let bytes = label.as_bytes();
    let len = bytes.len();

    if !(MIN_LABEL_LENGTH..=MAX_LABEL_LENGTH).contains(&len) {
        return Err(NameServiceError::InvalidLength(
            len,
            MIN_LABEL_LENGTH,
            MAX_LABEL_LENGTH,
        ));
    }

    for (i, character) in bytes.iter().enumerate() {
        let is_valid_character = match character {
            b'a'..=b'z' => true,
            b'0'..=b'9' => true,
            b'-' if i != 0 && i != len - 1 => true,
            _ => false,
        };

        if !is_valid_character {
            match character {
                b'-' => return Err(NameServiceError::InvalidHyphens),
                _ => return Err(NameServiceError::InvalidUnderscore),
            }
        };
    }
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parent_extraction() {
        let mut name = Domain::from_str("leaf.node.test.sui").unwrap();

        assert_eq!(name.parent().to_string(), "node.test.sui");

        name = Domain::from_str("node.test.sui").unwrap();

        assert_eq!(name.parent().to_string(), "test.sui");
    }

    #[test]
    fn test_expirations() {
        let system_time: u64 = 100;

        let mut name = NameRecord {
            nft_id: sui_types::id::ID::new(ObjectID::random()),
            data: VecMap { contents: vec![] },
            target_address: Some(SuiAddress::random_for_testing_only()),
            expiration_timestamp_ms: system_time + 10,
        };

        assert!(!name.is_node_expired(system_time));

        name.expiration_timestamp_ms = system_time - 10;

        assert!(name.is_node_expired(system_time));
    }

    #[test]
    fn test_name_service_outputs() {
        assert_eq!("@test".parse::<Domain>().unwrap().to_string(), "test.sui");
        assert_eq!(
            "test.sui".parse::<Domain>().unwrap().to_string(),
            "test.sui"
        );
        assert_eq!(
            "test@sld".parse::<Domain>().unwrap().to_string(),
            "test.sld.sui"
        );
        assert_eq!(
            "test.test@example".parse::<Domain>().unwrap().to_string(),
            "test.test.example.sui"
        );
        assert_eq!(
            "sui@sui".parse::<Domain>().unwrap().to_string(),
            "sui.sui.sui"
        );

        assert_eq!("@sui".parse::<Domain>().unwrap().to_string(), "sui.sui");

        assert_eq!(
            "test*test@test".parse::<Domain>().unwrap().to_string(),
            "test.test.test.sui"
        );
        assert_eq!(
            "test.test.sui".parse::<Domain>().unwrap().to_string(),
            "test.test.sui"
        );
        assert_eq!(
            "test.test.test.sui".parse::<Domain>().unwrap().to_string(),
            "test.test.test.sui"
        );
    }

    #[test]
    fn test_different_wildcard() {
        assert_eq!("test.sui".parse::<Domain>(), "test*sui".parse::<Domain>(),);

        assert_eq!("@test".parse::<Domain>(), "test*sui".parse::<Domain>(),);
    }

    #[test]
    fn test_invalid_inputs() {
        assert!("*".parse::<Domain>().is_err());
        assert!(".".parse::<Domain>().is_err());
        assert!("@".parse::<Domain>().is_err());
        assert!("@inner.sui".parse::<Domain>().is_err());
        assert!("@inner*sui".parse::<Domain>().is_err());
        assert!("test@".parse::<Domain>().is_err());
        assert!("sui".parse::<Domain>().is_err());
        assert!("test.test@example.sui".parse::<Domain>().is_err());
        assert!("test@test@example".parse::<Domain>().is_err());
    }

    #[test]
    fn output_tests() {
        let mut domain = "test.sui".parse::<Domain>().unwrap();
        assert!(domain.format(DomainFormat::Dot) == "test.sui");
        assert!(domain.format(DomainFormat::At) == "@test");

        domain = "test.test.sui".parse::<Domain>().unwrap();
        assert!(domain.format(DomainFormat::Dot) == "test.test.sui");
        assert!(domain.format(DomainFormat::At) == "test@test");

        domain = "test.test.test.sui".parse::<Domain>().unwrap();
        assert!(domain.format(DomainFormat::Dot) == "test.test.test.sui");
        assert!(domain.format(DomainFormat::At) == "test.test@test");

        domain = "test.test.test.test.sui".parse::<Domain>().unwrap();
        assert!(domain.format(DomainFormat::Dot) == "test.test.test.test.sui");
        assert!(domain.format(DomainFormat::At) == "test.test.test@test");
    }
}
