// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;

use bytes::Bytes;
use move_core_types::language_storage::StructTag;
use prost::Message;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::owner::OwnerKind;
use sui_sdk_types::Address;
use sui_types::storage::FilterLiteral;
use sui_types::storage::OwnedObjectFilter;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::OwnedObjectKey;
use sui_types::storage::OwnedObjectOwner;
use sui_types::storage::StructTypePattern;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::render::RenderContext;
use crate::reader::indexes_disabled;

use super::checkpoint::PinSource;
use super::checkpoint::resolve_pinned;
use super::checkpoint::resolve_state_read;
use super::cursor::DecodedPageToken;
use super::cursor::StateCursor;
use super::cursor::StateCursorKey;
use super::cursor::decode_page_token;
use super::cursor::parse_query_options;
use super::cursor::reject_legacy_paging_with_options;
use super::cursor::wrong_cursor_scope;
use super::filter::owned_object_filter;
use super::filter::truncate_echo;

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OWNED_OBJECT;

#[tracing::instrument(skip(service))]
pub fn list_owned_objects(
    service: &RpcService,
    request: ListOwnedObjectsRequest,
) -> Result<ListOwnedObjectsResponse> {
    let reader = service.reader.inner();
    reader.indexes().ok_or_else(indexes_disabled)?;

    let owner = parse_owner(request.owner_kind, request.owner.as_deref())?;

    let object_type: Option<StructTypePattern> = request
        .object_type
        .as_deref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e: anyhow::Error| {
            FieldViolation::new("object_type")
                .with_description(format!(
                    "invalid object_type: {}",
                    truncate_echo(&e.to_string())
                ))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let filter = match (&object_type, &request.filter) {
        (Some(_), Some(_)) => {
            return Err(FieldViolation::new("filter")
                .with_description("set at most one of `object_type` and `filter`")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
        (Some(pattern), None) => Some(OwnedObjectFilter {
            terms: vec![vec![FilterLiteral {
                negated: false,
                predicate: pattern.clone(),
            }]],
        }),
        (None, Some(filter)) => Some(owned_object_filter(filter)?),
        (None, None) => None,
    };

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
    let should_load = should_load_object(&read_mask);

    let extract_key = |field: &str, key: StateCursorKey| match key {
        StateCursorKey::OwnedObjects {
            owner: cur_owner,
            key,
        } if cur_owner == owner => Ok(key),
        _ => Err(wrong_cursor_scope(
            field,
            "cursor does not match this ListOwnedObjects owner",
        )),
    };

    if let Some(options) = request.options.as_ref() {
        reject_legacy_paging_with_options(request.page_size, request.page_token.as_deref())?;
        let opts = parse_query_options(
            Some(options),
            request.at_checkpoint,
            DEFAULT_PAGE_SIZE,
            MAX_PAGE_SIZE,
            extract_key,
        )?;
        let pinned = resolve_pinned(reader.as_ref(), opts.checkpoint, opts.pin_source, true)?;

        let (lower, upper) = opts.bounds();
        let mut iter = pinned.indexes.owned_objects_iter(
            owner,
            filter.as_ref(),
            lower,
            upper,
            opts.descending,
        )?;
        let mut objects = Vec::with_capacity(opts.limit);
        let mut cursors = Vec::with_capacity(opts.limit);
        let render_context = RenderContext::empty(reader);
        let mut size_bytes = 0;
        let mut hit_limit = false;

        while let Some(object_info) = iter
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        {
            let object = if should_load {
                let Some(object) =
                    reader.get_object_by_key(&object_info.object_id, object_info.version)
                else {
                    return Err(pinned.missing_object_error(
                        reader.as_ref(),
                        object_info.object_id,
                        object_info.version,
                    ));
                };
                service.render_object_to_proto(&object, &read_mask, &render_context)
            } else {
                owned_object_to_proto(&object_info, &read_mask)
            };

            let cursor = StateCursor {
                checkpoint: pinned.checkpoint,
                key: StateCursorKey::OwnedObjects {
                    owner,
                    key: OwnedObjectKey {
                        object_type: object_info.object_type,
                        balance: object_info.balance,
                        object_id: object_info.object_id,
                    },
                },
            }
            .encode();

            size_bytes += object.encoded_len();
            objects.push(object);
            cursors.push(cursor);

            if objects.len() >= opts.limit || size_bytes >= MAX_PAGE_SIZE_BYTES {
                hit_limit = true;
                break;
            }
        }

        // Re-check object pruning floor after reads to catch pruning that raced with object loads.
        pinned.check_object_floor(reader.as_ref())?;

        let end_reason = if hit_limit {
            QueryEndReason::ItemLimit
        } else {
            opts.exhausted_end_reason()
        };
        let mut message = ListOwnedObjectsResponse::default();
        message.objects = objects;
        message.cursors = cursors;
        message.end = Some(QueryEnd::default().with_reason(end_reason));
        return Ok(message);
    }

    // Non-ADDRESS owner kinds require cursor paging (`options`) or a checkpoint-pinned read
    // (`at_checkpoint` or a pinned `page_token`).
    if !matches!(owner, OwnedObjectOwner::Address(_))
        && request.at_checkpoint.is_none()
        && request.page_token.is_none()
    {
        return Err(FieldViolation::new("owner_kind")
            .with_description("owner_kind other than ADDRESS requires options or at_checkpoint")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    let legacy_struct_tag: Option<StructTag> = match &object_type {
        Some(StructTypePattern::Type(tag)) => Some(tag.clone()),
        _ => None,
    };

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let page_token = request
        .page_token
        .as_deref()
        .map(|token| {
            decode_page_token::<LegacyPageToken, OwnedObjectKey, _, _>(
                token,
                request.at_checkpoint,
                |t| {
                    matches!(owner, OwnedObjectOwner::Address(addr) if Address::from(addr) == t.owner)
                        && t.object_type == legacy_struct_tag
                },
                extract_key,
            )
        })
        .transpose()?;

    let (checkpoint, pin, pin_source, lower) = match page_token {
        Some(DecodedPageToken::Legacy(token)) => (
            None,
            false,
            PinSource::AtCheckpoint,
            Bound::Included(OwnedObjectKey {
                object_type: token.inner.object_type,
                balance: token.inner.balance,
                object_id: token.inner.object_id,
            }),
        ),
        Some(DecodedPageToken::Pinned { checkpoint, key }) => (
            Some(checkpoint),
            true,
            if request.at_checkpoint.is_some() {
                PinSource::AtCheckpoint
            } else {
                PinSource::Cursor
            },
            Bound::Excluded(key),
        ),
        None => (
            request.at_checkpoint,
            request.at_checkpoint.is_some(),
            PinSource::AtCheckpoint,
            Bound::Unbounded,
        ),
    };

    let target = resolve_state_read(service, checkpoint, pin, pin_source, true)?;

    let mut iter =
        target
            .indexes()
            .owned_objects_iter(owner, filter.as_ref(), lower, Bound::Unbounded, false)?;
    let mut objects = Vec::with_capacity(page_size);
    let render_context = RenderContext::empty(reader);
    let mut size_bytes = 0;
    let mut last_key: Option<OwnedObjectKey> = None;

    while let Some(object_info) = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    {
        let object = if should_load {
            let Some(object) =
                reader.get_object_by_key(&object_info.object_id, object_info.version)
            else {
                if let Some(pinned) = target.pinned() {
                    return Err(pinned.missing_object_error(
                        reader.as_ref(),
                        object_info.object_id,
                        object_info.version,
                    ));
                }
                tracing::debug!(
                    "unable to find object {}:{} while iterating through owned objects",
                    object_info.object_id,
                    object_info.version
                );
                continue;
            };

            service.render_object_to_proto(&object, &read_mask, &render_context)
        } else {
            owned_object_to_proto(&object_info, &read_mask)
        };

        last_key = Some(OwnedObjectKey {
            object_type: object_info.object_type,
            balance: object_info.balance,
            object_id: object_info.object_id,
        });
        size_bytes += object.encoded_len();
        objects.push(object);

        if objects.len() >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }
    }

    let next_row = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    if let Some(pinned) = target.pinned() {
        pinned.check_object_floor(reader.as_ref())?;
    }

    let next_page_token = next_row.map(|cursor| {
        if let Some(pinned) = target.pinned() {
            StateCursor {
                checkpoint: pinned.checkpoint,
                key: StateCursorKey::OwnedObjects {
                    owner,
                    key: last_key.expect("next row only peeked after non-empty page"),
                },
            }
            .encode()
        } else {
            let OwnedObjectOwner::Address(addr) = owner else {
                unreachable!("non-ADDRESS owner kinds are always pinned");
            };
            encode_legacy_page_token(LegacyPageToken {
                owner: addr.into(),
                object_type: legacy_struct_tag,
                inner: cursor,
            })
        }
    });

    let mut message = ListOwnedObjectsResponse::default();
    message.objects = objects;
    message.next_page_token = next_page_token;
    Ok(message)
}

fn parse_owner(owner_kind: Option<i32>, owner: Option<&str>) -> Result<OwnedObjectOwner> {
    let kind = match owner_kind {
        None => OwnerKind::Address,
        Some(raw) => match OwnerKind::try_from(raw) {
            Ok(OwnerKind::Address) => OwnerKind::Address,
            Ok(OwnerKind::Object) => OwnerKind::Object,
            Ok(OwnerKind::Shared) => OwnerKind::Shared,
            Ok(OwnerKind::Immutable) => OwnerKind::Immutable,
            Ok(_) | Err(_) => {
                return Err(FieldViolation::new("owner_kind")
                    .with_description(
                        "owner_kind must be one of ADDRESS, OBJECT, SHARED, or IMMUTABLE",
                    )
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        },
    };

    match kind {
        OwnerKind::Address => {
            let addr: Address = owner
                .ok_or_else(|| FieldViolation::new("owner").with_reason(ErrorReason::FieldMissing))?
                .parse()
                .map_err(|e| {
                    FieldViolation::new("owner")
                        .with_description(format!("invalid owner: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                })?;
            Ok(OwnedObjectOwner::Address(addr.into()))
        }
        OwnerKind::Object => {
            let addr: Address = owner
                .ok_or_else(|| FieldViolation::new("owner").with_reason(ErrorReason::FieldMissing))?
                .parse()
                .map_err(|e| {
                    FieldViolation::new("owner")
                        .with_description(format!("invalid owner: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                })?;
            Ok(OwnedObjectOwner::Object(addr.into()))
        }
        OwnerKind::Shared => {
            if owner.is_some() {
                return Err(FieldViolation::new("owner")
                    .with_description("owner must be unset when owner_kind is SHARED")
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
            Ok(OwnedObjectOwner::Shared)
        }
        OwnerKind::Immutable => {
            if owner.is_some() {
                return Err(FieldViolation::new("owner")
                    .with_description("owner must be unset when owner_kind is IMMUTABLE")
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
            Ok(OwnedObjectOwner::Immutable)
        }
        _ => unreachable!(),
    }
}

fn encode_legacy_page_token(page_token: LegacyPageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

fn owned_object_to_proto(info: &OwnedObjectInfo, mask: &FieldMaskTree) -> Object {
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

/// Does not record `filter`: resuming with a different filter continues from
/// the same index position.
#[derive(serde::Serialize, serde::Deserialize)]
struct LegacyPageToken {
    owner: Address,
    object_type: Option<StructTag>,
    inner: OwnedObjectInfo,
}

pub(super) fn should_load_object(mask: &FieldMaskTree) -> bool {
    [
        Object::BCS_FIELD,
        Object::DIGEST_FIELD,
        Object::OWNER_FIELD,
        Object::HAS_PUBLIC_TRANSFER_FIELD,
        Object::CONTENTS_FIELD,
        // Object::PACKAGE_FIELD, live index objects are Move struct objects, never packages
        Object::PREVIOUS_TRANSACTION_FIELD,
        Object::STORAGE_REBATE_FIELD,
        Object::JSON_FIELD,
        Object::DISPLAY_FIELD,
    ]
    .into_iter()
    .any(|field| mask.contains(field))
}
