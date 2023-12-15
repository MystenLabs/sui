// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use serde::{Deserialize, Serialize};
use sui_json_rpc::name_service::Domain;

use super::move_object::MoveObject;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SuinsRegistration {
    pub id: sui_types::id::UID,
    pub domain: Domain,
    pub domain_name: String,
    pub expiration_timestamp_ms: u64,
    pub image_url: String,
}

#[derive(Clone)]
pub(crate) struct NameServiceName {
    /// Representation of this NameServiceName as a generic Move object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move object's contents.
    pub native: SuinsRegistration,
}

pub(crate) enum NameServiceNameDowncastError {
    NotANameServiceName,
    Bcs(bcs::Error),
}

#[Object]
impl NameServiceName {
    /// Domain name of the NameServiceName object
    async fn domain(&self) -> &str {
        &self.native.domain_name
    }

    /// Convert the NameServiceName object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl TryFrom<&MoveObject> for NameServiceName {
    type Error = NameServiceNameDowncastError;

    fn try_from(move_object: &MoveObject) -> Result<Self, Self::Error> {
        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(NameServiceNameDowncastError::Bcs)?,
        })
    }
}
