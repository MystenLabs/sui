// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};
use sui_types::id::UID;

use super::move_object::MoveObject;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Domain {
    labels: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NativeSuinsRegistration {
    pub id: UID,
    pub domain: Domain,
    pub domain_name: String,
    pub expiration_timestamp_ms: u64,
    pub image_url: String,
}

#[derive(Clone)]
pub(crate) struct SuinsRegistration {
    /// Representation of this SuinsRegistration as a generic Move object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move object's contents.
    pub native: NativeSuinsRegistration,
}

pub(crate) enum SuinsRegistrationDowncastError {
    NotASuinsRegistration,
    Bcs(bcs::Error),
}

impl SuinsRegistration {
    // Because the type of the SuinsRegistration object is not constant,
    // we need to take it in as a param.
    pub fn try_from(
        move_object: &MoveObject,
        tag: &StructTag,
    ) -> Result<Self, SuinsRegistrationDowncastError> {
        if !move_object.native.is_type(tag) {
            return Err(SuinsRegistrationDowncastError::NotASuinsRegistration);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(SuinsRegistrationDowncastError::Bcs)?,
        })
    }
}

#[Object]
impl SuinsRegistration {
    /// Domain name of the SuinsRegistration object
    async fn domain(&self) -> &str {
        &self.native.domain_name
    }

    /// Convert the SuinsRegistration object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}
