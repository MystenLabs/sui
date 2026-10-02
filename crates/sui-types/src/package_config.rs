// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{ObjectID, SequenceNumber};
use crate::dynamic_field::DynamicFieldKey;
use crate::error::SuiResult;
use crate::storage::RuntimeObjectResolver;
use crate::{MoveTypeTagTrait, SUI_FRAMEWORK_PACKAGE_ID, SUI_PACKAGE_CONFIG_OBJECT_ID, id::ID};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::{Deserialize, Serialize};

pub const PACKAGE_CONFIG_MODULE_NAME: &IdentStr = ident_str!("package_config");

/// Rust representation of the Move type 0x2::package_config::VersionForbiddenKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionForbiddenKey {
    original_id: ObjectID,
    version: u64,
}

impl VersionForbiddenKey {
    pub fn new(original_id: ObjectID, version: u64) -> Self {
        Self {
            original_id,
            version,
        }
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: PACKAGE_CONFIG_MODULE_NAME.to_owned(),
            name: ident_str!("VersionForbiddenKey").to_owned(),
            type_params: vec![],
        }
    }
}

impl MoveTypeTagTrait for VersionForbiddenKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}

/// Rust representation of the Move type 0x2::package_config::MinVersionKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinVersionKey(ObjectID);

impl MinVersionKey {
    pub fn new(original_id: ObjectID) -> Self {
        Self(original_id)
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: PACKAGE_CONFIG_MODULE_NAME.to_owned(),
            name: ident_str!("MinVersionKey").to_owned(),
            type_params: vec![],
        }
    }
}

impl MoveTypeTagTrait for MinVersionKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}

/// Rust representation of the Move type 0x2::package_config::MinVersion.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MinVersion {
    pub version: u64,
    pub package_id: ID,
}

fn read_field<K, V>(
    key: K,
    key_type: TypeTag,
    root_version: SequenceNumber,
    object_store: &dyn RuntimeObjectResolver,
) -> SuiResult<Option<V>>
where
    K: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug,
    V: Serialize + for<'de> Deserialize<'de>,
{
    DynamicFieldKey(SUI_PACKAGE_CONFIG_OBJECT_ID, key, key_type)
        .into_id_with_bound(root_version)?
        .load_object(object_store)?
        .map(|field| field.load_value())
        .transpose()
}

/// Reads the minversion selection at `root_version`. Missing fields return `Ok(None)`; malformed
/// fields return an error so execution can fail closed.
pub fn read_minversion(
    original_id: ObjectID,
    root_version: SequenceNumber,
    object_store: &dyn RuntimeObjectResolver,
) -> SuiResult<Option<MinVersion>> {
    let key = MinVersionKey::new(original_id);
    read_field(
        key,
        MinVersionKey::get_type_tag(),
        root_version,
        object_store,
    )
}

/// Returns whether `package_version` is forbidden for `original_id` at `root_version`. Missing
/// fields are allowed. Unknown non-zero values fail closed.
pub fn is_version_forbidden(
    original_id: ObjectID,
    package_version: u64,
    root_version: SequenceNumber,
    object_store: &dyn RuntimeObjectResolver,
) -> SuiResult<bool> {
    let key = VersionForbiddenKey::new(original_id, package_version);
    Ok(read_field(
        key,
        VersionForbiddenKey::get_type_tag(),
        root_version,
        object_store,
    )?
    .is_some_and(|value: u64| value != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        digests::TransactionDigest,
        dynamic_field::DynamicFieldKey,
        error::SuiErrorKind,
        in_memory_storage::InMemoryStorage,
        object::{Object, Owner},
    };

    const ROOT_VERSION: SequenceNumber = SequenceNumber::from_u64(1);

    #[test]
    fn missing_policy_fields_are_allowed() {
        let store = InMemoryStorage::default();
        let original_id = ObjectID::random();

        assert_eq!(
            read_minversion(original_id, ROOT_VERSION, &store).unwrap(),
            None
        );
        assert!(!is_version_forbidden(original_id, 1, ROOT_VERSION, &store).unwrap());
    }

    #[test]
    fn malformed_policy_fields_fail_closed() {
        let original_id = ObjectID::random();
        let minversion = DynamicFieldKey(
            SUI_PACKAGE_CONFIG_OBJECT_ID,
            MinVersionKey::new(original_id),
            MinVersionKey::get_type_tag(),
        )
        .into_field(0u64)
        .unwrap()
        .into_move_object_unsafe_for_testing(ROOT_VERSION)
        .unwrap();
        let forbidden = DynamicFieldKey(
            SUI_PACKAGE_CONFIG_OBJECT_ID,
            VersionForbiddenKey::new(original_id, 1),
            VersionForbiddenKey::get_type_tag(),
        )
        .into_field(ObjectID::random())
        .unwrap()
        .into_move_object_unsafe_for_testing(ROOT_VERSION)
        .unwrap();
        let store = InMemoryStorage::new(vec![
            Object::new_move(
                minversion,
                Owner::ObjectOwner(SUI_PACKAGE_CONFIG_OBJECT_ID.into()),
                TransactionDigest::genesis_marker(),
            ),
            Object::new_move(
                forbidden,
                Owner::ObjectOwner(SUI_PACKAGE_CONFIG_OBJECT_ID.into()),
                TransactionDigest::genesis_marker(),
            ),
        ]);

        assert!(matches!(
            *read_minversion(original_id, ROOT_VERSION, &store)
                .unwrap_err()
                .0,
            SuiErrorKind::DynamicFieldReadError(_)
        ));
        assert!(matches!(
            *is_version_forbidden(original_id, 1, ROOT_VERSION, &store)
                .unwrap_err()
                .0,
            SuiErrorKind::DynamicFieldReadError(_)
        ));
    }
}
