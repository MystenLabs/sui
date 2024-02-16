// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{accept::AcceptFormat, response::ResponseContent, types::JsonObject, Result};
use axum::extract::{Path, State};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    object::Object,
    storage::ReadStore,
};
use tap::Pipe;

pub const GET_OBJECT_PATH: &str = "/objects/:object_id";

pub async fn get_object<S: ReadStore>(
    Path(object_id): Path<ObjectID>,
    accept: AcceptFormat,
    State(state): State<S>,
) -> Result<ResponseContent<Object, JsonObject>> {
    let object = state
        .get_object(&object_id)?
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(JsonObject::from_object(&object)),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
}

pub const GET_OBJECT_WITH_VERSION_PATH: &str = "/objects/:object_id/version/:version";

pub async fn get_object_with_version<S: ReadStore>(
    Path((object_id, version)): Path<(ObjectID, SequenceNumber)>,
    accept: AcceptFormat,
    State(state): State<S>,
) -> Result<ResponseContent<Object, JsonObject>> {
    let object = state
        .get_object_by_key(&object_id, version)?
        .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(JsonObject::from_object(&object)),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
}

#[derive(Debug)]
pub struct ObjectNotFoundError {
    object_id: ObjectID,
    version: Option<SequenceNumber>,
}

impl ObjectNotFoundError {
    pub fn new(object_id: ObjectID) -> Self {
        Self {
            object_id,
            version: None,
        }
    }

    pub fn new_with_version(object_id: ObjectID, version: SequenceNumber) -> Self {
        Self {
            object_id,
            version: Some(version),
        }
    }
}

impl std::fmt::Display for ObjectNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object {}", self.object_id)?;

        if let Some(version) = self.version {
            write!(f, " with version {version}")?;
        }

        write!(f, " not found")
    }
}

impl std::error::Error for ObjectNotFoundError {}

impl From<ObjectNotFoundError> for crate::RestError {
    fn from(value: ObjectNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}
