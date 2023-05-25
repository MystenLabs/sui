// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::collection_types::VecMap;
use sui_types::id::ID;

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

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct DomainParseError;

impl FromStr for Domain {
    type Err = DomainParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        /// The maximum length of a full domain
        const MAX_DOMAIN_LENGTH: usize = 200;

        if s.len() > MAX_DOMAIN_LENGTH {
            return Err(DomainParseError);
        }

        let labels = s
            .split('.')
            .rev()
            .map(validate_label)
            .collect::<Result<Vec<_>, Self::Err>>()?;

        if labels.is_empty() {
            return Err(DomainParseError);
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
        return Err(DomainParseError);
    }

    for (i, character) in bytes.iter().enumerate() {
        let is_valid_character = match character {
            b'a'..=b'z' => true,
            b'0'..=b'9' => true,
            b'-' if i != 0 && i != len - 1 => true,
            _ => false,
        };

        if !is_valid_character {
            return Err(DomainParseError);
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
struct NameRecord {
    /// The ID of the `RegistrationNFT` assigned to this record.
    ///
    /// The owner of the corrisponding `RegistrationNFT` has the rights to
    /// be able to change and adjust the `target_address` of this domain.
    ///
    /// It is possible that the ID changes if the record expires and is
    /// purchased by someone else.
    nft_id: ID,
    /// Timestamp in milliseconds when the record expires.
    expiration_timestamp_ms: u64,
    /// The target address that this domain points to
    target_address: Option<SuiAddress>,
    /// Additional data which may be stored in a record
    data: VecMap<String, String>,
}
