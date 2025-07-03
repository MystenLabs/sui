// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use prost::Message;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::merge::Merge;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2beta2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2beta2::ListOwnedObjectsResponse;
use sui_rpc::proto::sui::rpc::v2beta2::Object;
use sui_sdk_types::Address;
use sui_types::storage::OwnedObjectInfo;

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = "object_id,version,object_type";

#[tracing::instrument(skip(service))]
pub fn list_owned_objects(
    service: &RpcService,
    request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

    let owner: Address = request
        .owner
        .as_ref()
        .ok_or_else(|| FieldViolation::new("owner").with_reason(ErrorReason::FieldMissing))?
        .parse()
        .map_err(|e| {
            FieldViolation::new("owner")
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let object_type = request
        .object_type
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| {
            FieldViolation::new("object_type")
                .with_description(format!("invalid object_type: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;
    if let Some(token) = &page_token {
        if token.owner != owner || token.object_type != object_type {
            return Err(FieldViolation::new("page_token")
                .with_description("invalid page_token")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let should_load_object = should_load_object(&read_mask);
    let mut iter = indexes.owned_objects_iter(
        owner.into(),
        object_type.clone(),
        page_token.map(|t| t.inner),
    )?;
    let mut objects = Vec::with_capacity(page_size);
    let mut size_bytes = 0;
    while let Some(object_info) = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    {
        let object = if should_load_object {
            let Some(object) = service
                .reader
                .inner()
                .get_object_by_key(&object_info.object_id, object_info.version)
            else {
                tracing::debug!(
                    "unable to find object {}:{} while iterating through owned objects",
                    object_info.object_id,
                    object_info.version
                );
                continue;
            };

            let mut object = Object::merge_from(object, &read_mask);
            if read_mask.contains(Object::BALANCE_FIELD) {
                object.balance = object_info.balance;
            }
            object
        } else {
            owned_object_to_proto(object_info, &read_mask)
        };

        size_bytes += object.encoded_len();
        objects.push(object);

        if objects.len() >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }
    }

    let next_page_token = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .map(|cursor| {
            encode_page_token(PageToken {
                owner,
                object_type,
                inner: cursor,
            })
        });

    Ok(ListOwnedObjectsResponse {
        objects,
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(|_| {
        FieldViolation::new("page_token")
            .with_description("invalid page_token")
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

fn owned_object_to_proto(info: OwnedObjectInfo, mask: &FieldMaskTree) -> Object {
    let mut message = Object::default();

    if mask.contains(Object::OBJECT_ID_FIELD) {
        message.object_id = Some(info.object_id.to_string());
    }
    if mask.contains(Object::VERSION_FIELD) {
        message.version = Some(info.version.value());
    }
    if mask.contains(Object::OBJECT_TYPE_FIELD) {
        message.object_type = Some(info.object_type.to_canonical_string(true));
    }
    if mask.contains(Object::BALANCE_FIELD) {
        message.balance = info.balance;
    }

    message
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    owner: Address,
    object_type: Option<move_core_types::language_storage::StructTag>,
    inner: OwnedObjectInfo,
}

fn should_load_object(mask: &FieldMaskTree) -> bool {
    [
        Object::BCS_FIELD,
        Object::DIGEST_FIELD,
        Object::OWNER_FIELD,
        Object::HAS_PUBLIC_TRANSFER_FIELD,
        Object::CONTENTS_FIELD,
        // Object::PACKAGE_FIELD, owned objects can't be packages
        Object::PREVIOUS_TRANSACTION_FIELD,
        Object::STORAGE_REBATE_FIELD,
        Object::JSON_FIELD,
    ]
    .into_iter()
    .any(|field| mask.contains(field))
}
