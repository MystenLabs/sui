// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

use crate::base_types::SequenceNumber;
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::{SuiError, SuiResult};
use crate::object::Owner;
use crate::storage::ObjectStore;
use crate::{id::UID, SUI_FRAMEWORK_ADDRESS, SUI_RANDOMNESS_STATE_OBJECT_ID};

pub const RANDOMNESS_MODULE_NAME: &IdentStr = ident_str!("random");
pub const RANDOMNESS_STATE_STRUCT_NAME: &IdentStr = ident_str!("Random");
pub const RANDOMNESS_STATE_UPDATE_FUNCTION_NAME: &IdentStr =
    ident_str!("update_randomness_state");
pub const RANDOMNESS_STATE_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");
pub const RESOLVED_SUI_RANDOMNESS_STATE: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    RANDOMNESS_MODULE_NAME,
    RANDOMNESS_STATE_STRUCT_NAME,
);

/// Current latest version of the randomness state object.
pub const RANDOMNESS_STATE_VERSION: u64 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Random {
    pub id: UID,
    pub version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RandomInner {
    pub version: u64,

    pub randomness_round: u64,
    pub random_bytes: Vec<u8>,
}

pub fn get_randomness_state(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<RandomInner>> {
    let outer = object_store.get_object(&SUI_RANDOMNESS_STATE_OBJECT_ID)?;
    let Some(outer) = outer else {
        return Ok(None);
    };
    let move_object = outer.data.try_as_move().ok_or_else(|| {
        SuiError::SuiSystemStateReadError("Random object must be a Move object".to_owned())
    })?;
    let outer = bcs::from_bytes::<Random>(move_object.contents())
        .map_err(|err| SuiError::SuiSystemStateReadError(err.to_string()))?;

    // No other versions exist yet.
    assert_eq!(outer.version, RANDOMNESS_STATE_VERSION);

    let id = outer.id.id.bytes;
    let inner: RandomInner =
        get_dynamic_field_from_store(object_store, id, &outer.version).map_err(|err| {
            SuiError::DynamicFieldReadError(format!(
            "Failed to load sui system state inner object with ID {id:?} and version {:?}: {err:?}",
            outer.version,
        ))
        })?;

    Ok(Some(inner))
}

pub fn get_randomness_state_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_RANDOMNESS_STATE_OBJECT_ID)?
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Randomness state object must be shared"),
        }))
}
