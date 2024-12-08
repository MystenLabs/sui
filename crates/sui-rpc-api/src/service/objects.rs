// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::GetObjectOptions;
use crate::types::ObjectResponse;
use crate::Result;
use crate::RpcService;
use sui_sdk_types::types::ObjectId;
use sui_sdk_types::types::Version;
use tap::Pipe;

impl RpcService {
    pub fn get_object(
        &self,
        object_id: ObjectId,
        version: Option<Version>,
        options: GetObjectOptions,
    ) -> Result<ObjectResponse> {
        let object = if let Some(version) = version {
            self.reader
                .get_object_with_version(object_id, version)?
                .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?
        } else {
            self.reader
                .get_object(object_id)?
                .ok_or_else(|| ObjectNotFoundError::new(object_id))?
        };

        let object_bcs = options
            .include_object_bcs()
            .then(|| bcs::to_bytes(&object))
            .transpose()?;

        ObjectResponse {
            object_id: object.object_id(),
            version: object.version(),
            digest: object.digest(),
            object: options.include_object().then_some(object),
            object_bcs,
        }
        .pipe(Ok)
    }
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

impl From<ObjectNotFoundError> for crate::RpcServiceError {
    fn from(value: ObjectNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}
