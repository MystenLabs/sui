// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::collection_types::VecMap;
use crate::id::{ID, UID};
use crate::SUI_FRAMEWORK_ADDRESS;
use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use serde::Deserialize;

pub const DISPLAY_MODULE_NAME: &IdentStr = ident_str!("display");
pub const DISPLAY_CREATED_EVENT_NAME: &IdentStr = ident_str!("DisplayCreated");
pub const DISPLAY_VERSION_UPDATED_EVENT_NAME: &IdentStr = ident_str!("VersionUpdated");

// TODO: add tests to keep in sync
/// Rust version of the Move sui::display::Display type
#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
pub struct DisplayObject {
    pub id: UID,
    pub fields: VecMap<String, String>,
    pub version: u16,
}

#[derive(Deserialize, Debug)]
/// The event that is emitted when a `Display` version is "released".
/// Serves for Display versioning.
pub struct DisplayVersionUpdatedEvent {
    pub id: UID,
    pub version: u16,
    pub fields: VecMap<String, String>,
}

impl DisplayVersionUpdatedEvent {
    pub fn type_(inner: &StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: DISPLAY_VERSION_UPDATED_EVENT_NAME.to_owned(),
            module: DISPLAY_MODULE_NAME.to_owned(),
            type_params: vec![inner.clone().into()],
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct DisplayCreatedEvent {
    // The Object ID of Display Object
    pub id: ID,
}

impl DisplayCreatedEvent {
    pub fn type_(inner: &StructTag) -> StructTag {
        StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            name: DISPLAY_CREATED_EVENT_NAME.to_owned(),
            module: DISPLAY_MODULE_NAME.to_owned(),
            type_params: vec![inner.clone().into()],
        }
    }
}
