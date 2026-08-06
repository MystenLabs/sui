// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{EpochId, ObjectID};
use crate::config::{Config, get_config_from_store, read_config_setting};
use crate::dynamic_field::DOFWrapper;
use crate::storage::ObjectStore;
use crate::{MoveTypeTagTrait, SUI_FRAMEWORK_PACKAGE_ID, SUI_PACKAGE_CONFIG_OBJECT_ID};
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use serde::{Deserialize, Serialize};

pub const PACKAGE_CONFIG_MODULE_NAME: &IdentStr = ident_str!("package_config");

// Bitset schema and flags -- this should track the constants and bit definitions in the Move
// module 0x2::package_config.
pub const BITSET_VERSION_SHIFT: u8 = 56;
pub const VERSION_FORBIDDEN: u64 = 1;

/// Rust representation of the Move type 0x2::package_config::PackageMetadataKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageMetadataKey(ObjectID);

impl PackageMetadataKey {
    pub fn new(original_id: ObjectID) -> Self {
        Self(original_id)
    }

    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_PACKAGE_ID.into(),
            module: PACKAGE_CONFIG_MODULE_NAME.to_owned(),
            name: ident_str!("PackageMetadataKey").to_owned(),
            type_params: vec![],
        }
    }
}

impl MoveTypeTagTrait for PackageMetadataKey {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Self::type_()))
    }
}

/// Rust representation of the Move type 0x2::package_config::VersionForbiddenKey.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionForbiddenKey(u64);

impl VersionForbiddenKey {
    pub fn new(version: u64) -> Self {
        Self(version)
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

/// Returns the per-package config for `original_id`, if it exists and can be decoded.
pub fn get_per_package_config(
    original_id: ObjectID,
    object_store: &dyn ObjectStore,
) -> Option<Config> {
    let config_key = DOFWrapper {
        name: PackageMetadataKey::new(original_id),
    };
    get_config_from_store(object_store, SUI_PACKAGE_CONFIG_OBJECT_ID, &config_key)
}

/// Reads flags for one package version. `None` means the config or setting is absent or could not
/// be decoded. `cur_epoch == None` reads the newer value; `Some(epoch)` reads the stable value for
/// that epoch.
pub fn read_version_forbid_flags(
    config: &Config,
    version: u64,
    object_store: &dyn ObjectStore,
    cur_epoch: Option<EpochId>,
) -> Option<u64> {
    read_config_setting(
        object_store,
        config,
        VersionForbiddenKey::new(version),
        cur_epoch,
    )
}

/// Returns whether flags deny a package version. Unknown bitset schema versions are denied.
pub fn flags_forbid_version(flags: u64) -> bool {
    (flags >> BITSET_VERSION_SHIFT) != 0 || (flags & VERSION_FORBIDDEN) != 0
}

/// Returns whether `version` is forbidden for `original_id`. Missing configuration is allowed.
pub fn is_version_forbidden(
    original_id: ObjectID,
    version: u64,
    object_store: &dyn ObjectStore,
    cur_epoch: Option<EpochId>,
) -> bool {
    get_per_package_config(original_id, object_store)
        .and_then(|config| read_version_forbid_flags(&config, version, object_store, cur_epoch))
        .is_some_and(flags_forbid_version)
}
