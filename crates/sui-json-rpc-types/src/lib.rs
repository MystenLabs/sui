// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Base58, Base64};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use balance_changes::*;
pub use object_changes::*;
use serde_with::serde_as;
pub use sui_checkpoint::*;
pub use sui_coin::*;
pub use sui_event::*;
pub use sui_extended::*;
pub use sui_governance::*;
pub use sui_move::*;
pub use sui_object::*;
pub use sui_protocol::*;
pub use sui_transaction::*;
use sui_types::base_types::ObjectID;

#[cfg(test)]
#[path = "unit_tests/rpc_types_tests.rs"]
mod rpc_types_tests;

mod balance_changes;
mod displays;
mod object_changes;
mod sui_checkpoint;
mod sui_coin;
mod sui_event;
mod sui_extended;
mod sui_governance;
mod sui_move;
mod sui_object;
mod sui_protocol;
mod sui_transaction;

pub type DynamicFieldPage = Page<DynamicFieldInfo, ObjectID>;
/// `next_cursor` points to the last item in the page;
/// Reading with `next_cursor` will start from the next item after `next_cursor` if
/// `next_cursor` is `Some`, otherwise it will start from the first item.
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T, C> {
    pub data: Vec<T>,
    pub next_cursor: Option<C>,
    pub has_next_page: bool,
}

impl<T, C> Page<T, C> {
    pub fn empty() -> Self {
        Self {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        }
    }
}

#[serde_with::serde_as]
#[derive(Clone, Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DynamicFieldInfo {
    pub name: sui_types::dynamic_field::DynamicFieldName,
    #[serde(flatten)]
    pub bcs_name: BcsName,
    pub type_: sui_types::dynamic_field::DynamicFieldType,
    pub object_type: String,
    pub object_id: ObjectID,
    pub version: sui_types::base_types::SequenceNumber,
    pub digest: sui_types::digests::ObjectDigest,
}

impl From<sui_types::dynamic_field::DynamicFieldInfo> for DynamicFieldInfo {
    fn from(
        sui_types::dynamic_field::DynamicFieldInfo {
            name,
            bcs_name,
            type_,
            object_type,
            object_id,
            version,
            digest,
        }: sui_types::dynamic_field::DynamicFieldInfo,
    ) -> Self {
        Self {
            name,
            bcs_name: BcsName::new(bcs_name),
            type_,
            object_type,
            object_id,
            version,
            digest,
        }
    }
}

#[serde_as]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "bcsEncoding")]
#[serde(from = "MaybeTaggedBcsName")]
pub enum BcsName {
    Base64 {
        #[serde_as(as = "Base64")]
        #[schemars(with = "Base64")]
        #[serde(rename = "bcsName")]
        bcs_name: Vec<u8>,
    },
    Base58 {
        #[serde_as(as = "Base58")]
        #[schemars(with = "Base58")]
        #[serde(rename = "bcsName")]
        bcs_name: Vec<u8>,
    },
}

impl BcsName {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self::Base64 { bcs_name: bytes }
    }

    pub fn bytes(&self) -> &[u8] {
        match self {
            BcsName::Base64 { bcs_name } => bcs_name.as_ref(),
            BcsName::Base58 { bcs_name } => bcs_name.as_ref(),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            BcsName::Base64 { bcs_name } => bcs_name,
            BcsName::Base58 { bcs_name } => bcs_name,
        }
    }
}

#[allow(unused)]
#[serde_as]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
enum MaybeTaggedBcsName {
    Tagged(TaggedBcsName),
    Base58 {
        #[serde_as(as = "Base58")]
        #[serde(rename = "bcsName")]
        bcs_name: Vec<u8>,
    },
}

#[serde_as]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "bcsEncoding")]
enum TaggedBcsName {
    Base64 {
        #[serde_as(as = "Base64")]
        #[serde(rename = "bcsName")]
        bcs_name: Vec<u8>,
    },
    Base58 {
        #[serde_as(as = "Base58")]
        #[serde(rename = "bcsName")]
        bcs_name: Vec<u8>,
    },
}

impl From<MaybeTaggedBcsName> for BcsName {
    fn from(name: MaybeTaggedBcsName) -> BcsName {
        let bcs_name = match name {
            MaybeTaggedBcsName::Tagged(TaggedBcsName::Base58 { bcs_name })
            | MaybeTaggedBcsName::Base58 { bcs_name } => bcs_name,
            MaybeTaggedBcsName::Tagged(TaggedBcsName::Base64 { bcs_name }) => bcs_name,
        };

        // Bytes are already decoded, force into Base64 variant to avoid serializing to base58
        Self::Base64 { bcs_name }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bcs_name_test() {
        let bytes = vec![0, 1, 2, 3, 4];
        let untagged_base58 = r#"{"bcsName":"12VfUX"}"#;
        let tagged_base58 = r#"{"bcsEncoding":"base58","bcsName":"12VfUX"}"#;
        let tagged_base64 = r#"{"bcsEncoding":"base64","bcsName":"AAECAwQ="}"#;

        println!(
            "{}",
            serde_json::to_string(&TaggedBcsName::Base64 {
                bcs_name: bytes.clone()
            })
            .unwrap()
        );

        assert_eq!(
            bytes,
            serde_json::from_str::<BcsName>(untagged_base58)
                .unwrap()
                .into_bytes()
        );
        assert_eq!(
            bytes,
            serde_json::from_str::<BcsName>(tagged_base58)
                .unwrap()
                .into_bytes()
        );
        assert_eq!(
            bytes,
            serde_json::from_str::<BcsName>(tagged_base64)
                .unwrap()
                .into_bytes()
        );

        // Roundtrip base64
        let name = serde_json::from_str::<BcsName>(tagged_base64).unwrap();
        let json = serde_json::to_string(&name).unwrap();
        let from_json = serde_json::from_str::<BcsName>(&json).unwrap();
        assert_eq!(name, from_json);

        // Roundtrip base58
        let name = serde_json::from_str::<BcsName>(tagged_base58).unwrap();
        let json = serde_json::to_string(&name).unwrap();
        let from_json = serde_json::from_str::<BcsName>(&json).unwrap();
        assert_eq!(name, from_json);
    }
}
