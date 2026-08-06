// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{EpochId, ObjectID};
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::storage::ObjectStore;
use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{MoveTypeTagTrait, SUI_FRAMEWORK_ADDRESS, id::UID};

pub const CONFIG_MODULE_NAME: &IdentStr = ident_str!("config");
pub const CONFIG_STRUCT_NAME: &IdentStr = ident_str!("Config");
pub const SETTING_STRUCT_NAME: &IdentStr = ident_str!("Setting");
pub const SETTING_DATA_STRUCT_NAME: &IdentStr = ident_str!("SettingData");
pub const RESOLVED_SUI_CONFIG: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    CONFIG_MODULE_NAME,
    CONFIG_STRUCT_NAME,
);

/// Rust representation of the Move type 0x2::config::Config.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub id: UID,
}

/// Rust representation of the Move type 0x2::config::Setting.
#[derive(Debug, Serialize, Deserialize)]
pub struct Setting<V> {
    pub data: Option<SettingData<V>>,
}

/// Rust representation of the Move type 0x2::config::SettingData.
#[derive(Debug, Serialize, Deserialize)]
pub struct SettingData<V> {
    pub newer_value_epoch: u64,
    pub newer_value: Option<V>,
    pub older_value_opt: Option<V>,
}

impl Config {
    pub fn type_() -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: CONFIG_MODULE_NAME.to_owned(),
            name: CONFIG_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}

pub fn setting_type(value_tag: TypeTag) -> StructTag {
    StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: CONFIG_MODULE_NAME.to_owned(),
        name: SETTING_STRUCT_NAME.to_owned(),
        type_params: vec![value_tag],
    }
}

impl MoveTypeTagTrait for Config {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(Config::type_()))
    }
}

impl<V: MoveTypeTagTrait> MoveTypeTagTrait for Setting<V> {
    fn get_type_tag() -> TypeTag {
        TypeTag::Struct(Box::new(setting_type(V::get_type_tag())))
    }
}

pub fn is_setting(tag: &StructTag) -> bool {
    let StructTag {
        address,
        module,
        name,
        type_params,
    } = tag;
    *address == SUI_FRAMEWORK_ADDRESS
        && module.as_ident_str() == CONFIG_MODULE_NAME
        && name.as_ident_str() == SETTING_STRUCT_NAME
        && type_params.len() == 1
}

/// Reads a `Config` from a dynamic field of `parent_id`.
///
/// Missing or invalid dynamic fields return `None`.
pub fn get_config_from_store<K>(
    object_store: &dyn ObjectStore,
    parent_id: ObjectID,
    key: &K,
) -> Option<Config>
where
    K: Clone + MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug,
{
    get_dynamic_field_from_store(object_store, parent_id, key).ok()
}

/// Reads a setting from `config`, returning the value applicable to `cur_epoch`.
///
/// Missing or invalid dynamic fields return `None`.
pub fn read_config_setting<K, V>(
    object_store: &dyn ObjectStore,
    config: &Config,
    setting_name: K,
    cur_epoch: Option<EpochId>,
) -> Option<V>
where
    K: Clone + MoveTypeTagTrait + Serialize + DeserializeOwned + fmt::Debug,
    V: Clone + Serialize + DeserializeOwned,
{
    let setting: Setting<V> =
        get_dynamic_field_from_store(object_store, *config.id.object_id(), &setting_name).ok()?;
    setting.read_value(cur_epoch).cloned()
}

impl<V> Setting<V> {
    /// Calls `SettingData::read_value` on the setting's data.
    /// The `data` should never be `None`, but for safety, this method returns `None` if it is.
    pub fn read_value(&self, cur_epoch: Option<EpochId>) -> Option<&V> {
        self.data.as_ref()?.read_value(cur_epoch)
    }
}

impl<V> SettingData<V> {
    /// Reads the value of the setting, giving `newer_value` if the current epoch is greater than
    /// `newer_value_epoch`, and `older_value_opt` otherwise.
    /// If `cur_epoch` is `None`, the `newer_value` is always returned.
    pub fn read_value(&self, cur_epoch: Option<EpochId>) -> Option<&V> {
        let use_newer_value = match cur_epoch {
            Some(cur_epoch) => cur_epoch > self.newer_value_epoch,
            None => true,
        };
        if use_newer_value {
            self.newer_value.as_ref()
        } else {
            self.older_value_opt.as_ref()
        }
    }
}
