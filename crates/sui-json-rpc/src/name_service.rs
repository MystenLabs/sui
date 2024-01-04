// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::collection_types::VecMap;
use sui_types::id::ID;
use sui_types::TypeTag;

const NAME_SERVICE_DOMAIN_MODULE: &IdentStr = ident_str!("domain");
const NAME_SERVICE_DOMAIN_STRUCT: &IdentStr = ident_str!("Domain");
const NAME_SERVICE_DEFAULT_PACKAGE_ADDRESS: &str =
    "0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0";
const NAME_SERVICE_DEFAULT_REGISTRY: &str =
    "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106";
const NAME_SERVICE_DEFAULT_REVERSE_REGISTRY: &str =
    "0x2fd099e17a292d2bc541df474f9fafa595653848cbabb2d7a4656ec786a1969f";
const _NAME_SERVICE_OBJECT_ADDRESS: &str =
    "0x6e0ddefc0ad98889c04bab9639e512c21766c5e6366f89e696956d9be6952871";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Registry {
    /// The `registry` table maps `Domain` to `NameRecord`.
    /// Added / replaced in the `add_record` function.
    registry: Table<Domain, NameRecord>,
    /// The `reverse_registry` table maps `address` to `domain_name`.
    /// Updated in the `set_reverse_lookup` function.
    reverse_registry: Table<SuiAddress, Domain>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Domain {
    labels: Vec<String>,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NameServiceConfig {
    pub package_address: SuiAddress,
    pub registry_id: ObjectID,
    pub reverse_registry_id: ObjectID,
    domain_type_tag: TypeTag,
}

impl NameServiceConfig {
    pub fn new(
        package_address: SuiAddress,
        registry_id: ObjectID,
        reverse_registry_id: ObjectID,
    ) -> Self {
        let domain_type_tag = Domain::type_(package_address);
        Self {
            package_address,
            registry_id,
            reverse_registry_id,
            domain_type_tag: TypeTag::Struct(Box::new(domain_type_tag)),
        }
    }

    pub fn record_field_id(&self, domain: &Domain) -> ObjectID {
        let domain_bytes = bcs::to_bytes(domain).unwrap();

        sui_types::dynamic_field::derive_dynamic_field_id(
            self.registry_id,
            &self.domain_type_tag,
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
}

impl Default for NameServiceConfig {
    fn default() -> Self {
        let package_address = SuiAddress::from_str(NAME_SERVICE_DEFAULT_PACKAGE_ADDRESS).unwrap();
        let registry_id = ObjectID::from_str(NAME_SERVICE_DEFAULT_REGISTRY).unwrap();
        let reverse_registry_id =
            ObjectID::from_str(NAME_SERVICE_DEFAULT_REVERSE_REGISTRY).unwrap();
        Self::new(package_address, registry_id, reverse_registry_id)
    }
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum DomainParseError {
    #[error("String length: {0} exceeds maximum allowed length: {1}")]
    ExceedsMaxLength(usize, usize),
    #[error("String length: {0} outside of valid range: [{1}, {2}]")]
    InvalidLength(usize, usize, usize),
    #[error("Hyphens are not allowed as the first or last character")]
    InvalidHyphens,
    #[error("Only lowercase letters, numbers, and hyphens are allowed")]
    InvalidUnderscore,
    #[error("Domain must contain at least one label")]
    LabelsEmpty,
}

impl FromStr for Domain {
    type Err = DomainParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// The maximum length of a full domain
        const MAX_DOMAIN_LENGTH: usize = 200;

        if s.len() > MAX_DOMAIN_LENGTH {
            return Err(DomainParseError::ExceedsMaxLength(
                s.len(),
                MAX_DOMAIN_LENGTH,
            ));
        }

        let labels = s
            .split('.')
            .rev()
            .map(validate_label)
            .collect::<Result<Vec<_>, Self::Err>>()?;

        if labels.is_empty() {
            return Err(DomainParseError::LabelsEmpty);
        }

        let labels = labels.into_iter().map(ToOwned::to_owned).collect();

        Ok(Domain { labels })
    }
}

fn validate_label(label: &str) -> Result<&str, DomainParseError> {
    const MIN_LABEL_LENGTH: usize = 1;
    const MAX_LABEL_LENGTH: usize = 63;
    let bytes = label.as_bytes();
    let len = bytes.len();

    if !(MIN_LABEL_LENGTH..=MAX_LABEL_LENGTH).contains(&len) {
        return Err(DomainParseError::InvalidLength(
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
                b'-' => return Err(DomainParseError::InvalidHyphens),
                _ => return Err(DomainParseError::InvalidUnderscore),
            }
        };
    }
    Ok(label)
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.labels.len();
        for (i, label) in self.labels.iter().rev().enumerate() {
            f.write_str(label)?;

            if i != len - 1 {
                f.write_str(".")?;
            }
        }
        Ok(())
    }
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
