// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto_zkp::bn254::zk_login::{JwkId, JWK};
use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};
use serde::{Deserialize, Serialize};

use crate::base_types::SequenceNumber;
use crate::dynamic_field::get_dynamic_field_from_store;
use crate::error::{SuiError, SuiResult};
use crate::object::Owner;
use crate::storage::ObjectStore;
use crate::{id::UID, SUI_AUTHENTICATOR_STATE_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};

pub const AUTHENTICATOR_STATE_MODULE_NAME: &IdentStr = ident_str!("authenticator_state");
pub const AUTHENTICATOR_STATE_STRUCT_NAME: &IdentStr = ident_str!("AuthenticatorState");
pub const AUTHENTICATOR_STATE_UPDATE_FUNCTION_NAME: &IdentStr =
    ident_str!("update_authenticator_state");
pub const AUTHENTICATOR_STATE_CREATE_FUNCTION_NAME: &IdentStr = ident_str!("create");
pub const AUTHENTICATOR_STATE_EXPIRE_JWKS_FUNCTION_NAME: &IdentStr = ident_str!("expire_jwks");
pub const RESOLVED_SUI_AUTHENTICATOR_STATE: (&AccountAddress, &IdentStr, &IdentStr) = (
    &SUI_FRAMEWORK_ADDRESS,
    AUTHENTICATOR_STATE_MODULE_NAME,
    AUTHENTICATOR_STATE_STRUCT_NAME,
);

/// Current latest version of the authenticator state object.
pub const AUTHENTICATOR_STATE_VERSION: u64 = 1;

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

fn string_bytes_ord(a: &str, b: &str) -> std::cmp::Ordering {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    if a_bytes.len() < b_bytes.len() {
        return std::cmp::Ordering::Less;
    }
    if a_bytes.len() > b_bytes.len() {
        return std::cmp::Ordering::Greater;
    }

    for (a_byte, b_byte) in a_bytes.iter().zip(b_bytes.iter()) {
        if a_byte < b_byte {
            return std::cmp::Ordering::Less;
        }
        if a_byte > b_byte {
            return std::cmp::Ordering::Greater;
        }
    }

    std::cmp::Ordering::Equal
}

// This must match the sort order defined by jwk_lt in authenticator_state.move
fn jwk_ord(a: &ActiveJwk, b: &ActiveJwk) -> std::cmp::Ordering {
    // note: epoch is ignored
    if a.jwk_id.iss != b.jwk_id.iss {
        string_bytes_ord(&a.jwk_id.iss, &b.jwk_id.iss)
    } else if a.jwk_id.kid != b.jwk_id.kid {
        string_bytes_ord(&a.jwk_id.kid, &b.jwk_id.kid)
    } else if a.jwk.kty != b.jwk.kty {
        string_bytes_ord(&a.jwk.kty, &b.jwk.kty)
    } else if a.jwk.e != b.jwk.e {
        string_bytes_ord(&a.jwk.e, &b.jwk.e)
    } else if a.jwk.n != b.jwk.n {
        string_bytes_ord(&a.jwk.n, &b.jwk.n)
    } else {
        string_bytes_ord(&a.jwk.alg, &b.jwk.alg)
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl std::cmp::PartialOrd for ActiveJwk {
    // This must match the sort order defined by jwk_lt in authenticator_state.move
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(jwk_ord(self, other))
    }
}

impl std::cmp::Ord for ActiveJwk {
    // This must match the sort order defined by jwk_lt in authenticator_state.move
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        jwk_ord(self, other)
    }
}

pub fn get_authenticator_state(
    object_store: impl ObjectStore,
) -> SuiResult<Option<AuthenticatorStateInner>> {
    let outer = object_store.get_object(&SUI_AUTHENTICATOR_STATE_OBJECT_ID);
    let Some(outer) = outer else {
        return Ok(None);
    };
    let move_object = outer.data.try_as_move().ok_or_else(|| {
        SuiError::SuiSystemStateReadError(
            "AuthenticatorState object must be a Move object".to_owned(),
        )
    })?;
    let outer = bcs::from_bytes::<AuthenticatorState>(move_object.contents())
        .map_err(|err| SuiError::SuiSystemStateReadError(err.to_string()))?;

    // No other versions exist yet.
    assert_eq!(outer.version, AUTHENTICATOR_STATE_VERSION);

    let id = outer.id.id.bytes;
    let inner: AuthenticatorStateInner =
        get_dynamic_field_from_store(&object_store, id, &outer.version).map_err(|err| {
            SuiError::DynamicFieldReadError(format!(
                "Failed to load sui system state inner object with ID {:?} and version {:?}: {:?}",
                id, outer.version, err
            ))
        })?;

    Ok(Some(inner))
}

pub fn get_authenticator_state_obj_initial_shared_version(
    object_store: &dyn ObjectStore,
) -> SuiResult<Option<SequenceNumber>> {
    Ok(object_store
        .get_object(&SUI_AUTHENTICATOR_STATE_OBJECT_ID)
        .map(|obj| match obj.owner {
            Owner::Shared {
                initial_shared_version,
            } => initial_shared_version,
            _ => unreachable!("Authenticator state object must be shared"),
        }))
}
