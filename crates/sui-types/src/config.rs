// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    account_address::AccountAddress,
    ident_str,
    identifier::IdentStr,
    language_storage::{StructTag, TypeTag},
};
use serde::{Deserialize, Serialize};

use crate::{id::UID, SUI_FRAMEWORK_ADDRESS};

pub const CONFIG_MODULE_NAME: &IdentStr = ident_str!("config");
pub const CONFIG_STRUCT_NAME: &IdentStr = ident_str!("Config");
pub const SETTING_STRUCT_NAME: &IdentStr = ident_str!("Setting");
pub const SETTING_DATA_STRUCT_NAME: &IdentStr = ident_str!("SettingData");
pub const RESOLVED_SUI_CONFIG: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    CONFIG_MODULE_NAME,
    CONFIG_STRUCT_NAME,
);

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub id: UID,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Setting<V> {
    pub id: UID,
    pub data: Option<SettingData<V>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettingData<V> {
    pub newer_value_epoch: u64,
    pub newer_value: V,
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

impl<V> Setting<V> {
    /// Calls `SettingData::read_value` on the setting's data.
    /// The `data` should never be `None`, but for safety, this method returns `None` if it is.
    pub fn read_value(&self, current_epoch: Option<u64>) -> Option<&V> {
        self.data.as_ref()?.read_value(current_epoch)
    }
}

impl<V> SettingData<V> {
    /// Reads the value of the setting, giving `newer_value` if the current epoch is greater than
    /// `newer_value_epoch`, and `older_value_opt` otherwise.
    /// If `current_epoch` is `None`, the `newer_value` is always returned.
    pub fn read_value(&self, current_epoch: Option<u64>) -> Option<&V> {
        let satisfies_epoch_bound = match current_epoch {
            Some(current_epoch) => current_epoch > self.newer_value_epoch,
            None => true,
        };
        if satisfies_epoch_bound {
            Some(&self.newer_value)
        } else {
            self.older_value_opt.as_ref()
        }
    }
}
