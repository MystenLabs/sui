// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::ObjectNotFoundError;
use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::node::v2::GetObjectRequest;
use crate::proto::node::v2::GetObjectResponse;
use crate::ErrorReason;
use crate::Result;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::ObjectId;
use tap::Pipe;

impl RpcService {
    pub fn get_object(
        &self,
        GetObjectRequest {
            object_id,
            version,
            read_mask,
        }: GetObjectRequest,
    ) -> Result<GetObjectResponse> {
        let object_id = object_id
            .ok_or_else(|| {
                FieldViolation::new("object_id")
                    .with_description("missing object_id")
                    .with_reason(ErrorReason::FieldMissing)
            })?
            .pipe_ref(ObjectId::try_from)
            .map_err(|e| {
                FieldViolation::new("object_id")
                    .with_description(format!("invalid object_id: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;

        let read_mask =
            read_mask.unwrap_or_else(|| FieldMask::from_str(GetObjectRequest::READ_MASK_DEFAULT));
        GetObjectResponse::validate_read_mask(&read_mask).map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let read_mask = FieldMaskTree::from(read_mask);

        let object = if let Some(version) = version {
            self.reader
                .get_object_with_version(object_id, version)?
                .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?
        } else {
            self.reader
                .get_object(object_id)?
                .ok_or_else(|| ObjectNotFoundError::new(object_id))?
        };

        let object_bcs = read_mask
            .contains("object_bcs")
            .then(|| bcs::to_bytes(&object))
            .transpose()?
            .map(Into::into);

        GetObjectResponse {
            object_id: read_mask
                .contains("object_id")
                .then(|| object.object_id().into()),
            version: read_mask.contains("version").then_some(object.version()),
            digest: read_mask.contains("digest").then(|| object.digest().into()),
            object: read_mask.contains("object").then(|| object.into()),
            object_bcs,
        }
        .pipe(Ok)
    }
}
