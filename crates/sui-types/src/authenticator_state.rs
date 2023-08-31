// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::{SuiError, SuiResult};
use crate::storage::ObjectStore;
use crate::{id::UID, SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};

pub const AUTHENTICATOR_STATE_MODULE_NAME: &IdentStr = ident_str!("authenticator_state");
pub const AUTHENTICATOR_STATE_STRUCT_NAME: &IdentStr = ident_str!("AuthenticatorState");
pub const RESOLVED_SUI_AUTHENTICATOR_STATE: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    AUTHENTICATOR_STATE_MODULE_NAME,
    AUTHENTICATOR_STATE_STRUCT_NAME,
);

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthenticatorState {
    pub id: UID,
    pub version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthenticatorStateInner {
    pub version: u64,

    /// List of currently active JWKs.
    pub active_jwks: Vec<ActiveJwk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ActiveJwk {
    pub jwk_id: JwkId,
    pub jwk: JWK,
    // the most recent epoch in which the jwk was validated
    pub epoch: u64,
}

pub fn get_authenticator_state(
    object_store: &dyn ObjectStore,
) -> SuiResult<AuthenticatorStateInner> {
    let outer = object_store
        .get_object(&SUI_AUTHENTICATOR_STATE_OBJECT_ID)?
        // Don't panic here on None because object_store is a generic store.
        .ok_or_else(|| {
            SuiError::SuiSystemStateReadError("AuthenticatorState object not found".to_owned())
        })?;
    let move_object = outer.data.try_as_move().ok_or_else(|| {
        SuiError::SuiSystemStateReadError(
            "AuthenticatorState object must be a Move object".to_owned(),
        )
    })?;
    let outer = bcs::from_bytes::<AuthenticatorState>(move_object.contents())
        .map_err(|err| SuiError::SuiSystemStateReadError(err.to_string()))?;

    // No other versions exist yet.
    assert_eq!(outer.version, 1);

    let id = outer.id.id.bytes;
    let inner: AuthenticatorStateInner =
        get_dynamic_field_from_store(object_store, id, &outer.version).map_err(|err| {
            SuiError::DynamicFieldReadError(format!(
                "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                id, outer.version, err
            ))
        })?;

    Ok(inner)
}
