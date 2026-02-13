// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::{context::Context, grpc::state_service::ForkingStateService, store::ForkingStore};

use base64::Engine;
use prost_types::FieldMask;
use sui_rpc::{
    field::{FieldMaskTree, FieldMaskUtil},
    proto::{
        google::rpc::bad_request::FieldViolation,
        sui::rpc::v2::{
            ListOwnedObjectsRequest, ListOwnedObjectsResponse, Object,
            state_service_server::StateService,
        },
    },
};
use sui_rpc_api::{ErrorReason, RpcError, proto::sui::rpc::v2 as grpc};
use sui_sdk_types::Address;
use sui_types::{base_types::SuiAddress, storage::OwnedObjectInfo};

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = "object_id,version,object_type";

pub type Result<T, E = RpcError> = std::result::Result<T, E>;

// TODO: finish this, it's mostly copied from /sui-rpc-api
pub fn list_owned_objects(
    service: &ForkingStateService,
    request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse> {
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
    if let Some(token) = &page_token
        && (token.owner != owner || token.object_type != object_type)
    {
        return Err(FieldViolation::new("page_token")
            .with_description("invalid page_token")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
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
    //
    // let sim = service.context.simulacrum.read().await;
    // let data_store = sim.store_static();
    //
    // let owner = request.into_inner().owner;
    // let Some(owner) = owner else {
    //     return Err(tonic::Status::invalid_argument("owner is required"));
    // };
    //
    // let sui_address = SuiAddress::from_str(&owner).unwrap();
    // let objects = data_store.owned_objects(sui_address);

    let mut message = ListOwnedObjectsResponse::default();
    message.objects = vec![];
    message.next_page_token = None;
    Ok(message)

    // let response = grpc::ListOwnedObjectsResponse {
    //     objects: objects
    //         .into_iter()
    //         .map(|object| Object {
    //             bcs: base64::engine::general_purpose::STANDARD::encode(
    //                 &bcs::to_bytes(&object).unwrap(),
    //             ),
    //             object_id: Some(object.id()),
    //             version: Some(object.version().into()),
    //             digest: Some(object.digest()),
    //             owner: Some(object.owner()),
    //             object_type: todo!(),
    //             has_public_transfer: todo!(),
    //             contents: todo!(),
    //             package: todo!(),
    //             previous_transaction: todo!(),
    //             storage_rebate: todo!(),
    //             json: todo!(),
    //             balance: todo!(),
    //             display: todo!(),
    //         })
    //         .collect(),
    //     next_page_token: todo!(),
    // };
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(|_| {
        FieldViolation::new("page_token")
            .with_description("invalid page_token")
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn encode_page_token(page_token: PageToken) -> bytes::Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

fn owned_object_to_proto(info: OwnedObjectInfo, mask: &FieldMaskTree) -> Object {
    let mut message = Object::default();

    if mask.contains(Object::OBJECT_ID_FIELD) {
        message.object_id = Some(info.object_id.to_canonical_string(true));
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
