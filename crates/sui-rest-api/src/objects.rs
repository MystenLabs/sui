// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{accept::AcceptFormat, reader::StateReader, response::ResponseContent, Result};
use axum::extract::{Path, State};
use sui_sdk2::types::{Object, ObjectId, Version};
use tap::Pipe;

pub const GET_OBJECT_PATH: &str = "/objects/:object_id";

pub async fn get_object(
    Path(object_id): Path<ObjectId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<Object>> {
    let object = state
        .get_object(object_id)?
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
}

pub const GET_OBJECT_WITH_VERSION_PATH: &str = "/objects/:object_id/version/:version";

pub async fn get_object_with_version(
    Path((object_id, version)): Path<(ObjectId, Version)>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<Object>> {
    let object = state
        .get_object_with_version(object_id, version)?
        .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
}

#[derive(Debug)]
pub struct ObjectNotFoundError {
    object_id: ObjectId,
    version: Option<Version>,
}

impl ObjectNotFoundError {
    pub fn new(object_id: ObjectId) -> Self {
        Self {
            object_id,
            version: None,
        }
    }

    pub fn new_with_version(object_id: ObjectId, version: Version) -> Self {
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
