// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;

use crate::grpc::state_service::ForkingStateService;

use prost::Message;
use prost_types::FieldMask;
use sui_rpc::{
    field::{FieldMaskTree, FieldMaskUtil},
    merge::Merge,
    proto::{
        google::rpc::bad_request::FieldViolation,
        sui::rpc::v2::{ListOwnedObjectsRequest, ListOwnedObjectsResponse, Object},
    },
};
use sui_rpc_api::{ErrorReason, RpcError};
use sui_sdk_types::Address;
use sui_types::{
    base_types::SuiAddress,
    object::{Object as SuiObject, Owner},
    storage::OwnedObjectInfo,
};

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = "object_id,version,object_type";

pub type Result<T, E = RpcError> = std::result::Result<T, E>;

pub async fn list_owned_objects(
    service: &ForkingStateService,
    request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse> {
    let owner_str = request
        .owner
        .as_ref()
        .ok_or_else(|| FieldViolation::new("owner").with_reason(ErrorReason::FieldMissing))?;
    let owner: Address = owner_str.parse().map_err(|e| {
        FieldViolation::new("owner")
            .with_description(format!("invalid owner: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let owner_sui: SuiAddress = owner_str.parse().map_err(|e| {
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
    let should_load_object = should_load_object(&read_mask);

    let mut candidates = {
        let sim = service.context.simulacrum.read().await;
        let store = sim.store();

        store
            .owned_objects(owner_sui)
            .filter_map(|object| owned_object_entry(object, owner_sui))
            .filter(|entry| {
                matches_object_type_filter(&entry.info.object_type, object_type.as_ref())
            })
            .collect::<Vec<_>>()
    };

    candidates.sort_by(|lhs, rhs| compare_owned_object_info(&lhs.info, &rhs.info));

    let start_index = page_token
        .as_ref()
        .map(|token| lower_bound_candidates(&candidates, &token.inner))
        .unwrap_or(0);

    let mut objects = Vec::with_capacity(page_size);
    let mut size_bytes = 0usize;
    let mut next_index = start_index;

    while let Some(entry) = candidates.get(next_index) {
        let object = if should_load_object {
            Object::merge_from(&entry.object, &read_mask)
        } else {
            owned_object_to_proto(entry.info.clone(), &read_mask)
        };

        size_bytes += object.encoded_len();
        objects.push(object);
        next_index += 1;

        if objects.len() >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }
    }

    let next_page_token = candidates.get(next_index).map(|entry| {
        encode_page_token(PageToken {
            owner,
            object_type: object_type.clone(),
            inner: entry.info.clone(),
        })
    });

    let mut message = ListOwnedObjectsResponse::default();
    message.objects = objects;
    message.next_page_token = next_page_token;
    Ok(message)
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

struct OwnedObjectEntry {
    info: OwnedObjectInfo,
    object: SuiObject,
}

fn owned_object_entry(object: SuiObject, expected_owner: SuiAddress) -> Option<OwnedObjectEntry> {
    let owner = match object.owner() {
        Owner::AddressOwner(owner) => *owner,
        Owner::ConsensusAddressOwner { owner, .. } => *owner,
        Owner::ObjectOwner(_) | Owner::Shared { .. } | Owner::Immutable => return None,
    };

    if owner != expected_owner {
        return None;
    }

    let object_type = object.struct_tag()?;
    let info = OwnedObjectInfo {
        owner,
        object_type,
        balance: object.as_coin_maybe().map(|coin| coin.balance.value()),
        object_id: object.id(),
        version: object.version(),
    };

    Some(OwnedObjectEntry { info, object })
}

fn matches_object_type_filter(
    object_type: &move_core_types::language_storage::StructTag,
    filter: Option<&move_core_types::language_storage::StructTag>,
) -> bool {
    filter
        .map(|filter| {
            filter.address == object_type.address
                && filter.module == object_type.module
                && filter.name == object_type.name
                && (filter.type_params.is_empty() || filter.type_params == object_type.type_params)
        })
        .unwrap_or(true)
}

fn compare_owned_object_info(lhs: &OwnedObjectInfo, rhs: &OwnedObjectInfo) -> Ordering {
    lhs.object_type
        .cmp(&rhs.object_type)
        .then_with(|| {
            lhs.balance
                .map(std::ops::Not::not)
                .cmp(&rhs.balance.map(std::ops::Not::not))
        })
        .then_with(|| lhs.object_id.cmp(&rhs.object_id))
}

fn lower_bound_candidates(candidates: &[OwnedObjectEntry], cursor: &OwnedObjectInfo) -> usize {
    candidates.partition_point(|entry| compare_owned_object_info(&entry.info, cursor).is_lt())
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
