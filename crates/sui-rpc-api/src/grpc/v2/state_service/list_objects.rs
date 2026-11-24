// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Bound;
use std::sync::Arc;

use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::ListObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_types::storage::ObjectByTypeInfo;
use sui_types::storage::ObjectByTypeKey;
use sui_types::storage::OwnedObjectFilter;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::render::RenderContext;
use crate::reader::indexes_disabled;

use super::checkpoint::PinnedStateRead;
use super::checkpoint::resolve_pinned;
use super::cursor::StateCursor;
use super::cursor::StateCursorKey;
use super::cursor::parse_query_options;
use super::cursor::wrong_cursor_scope;
use super::filter::owned_object_filter;
use super::list_owned_objects::should_load_object;

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 1000;
const CHUNK_SIZE: usize = 64;
const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OWNED_OBJECT;

#[tracing::instrument(skip(service))]
pub(super) async fn list_objects(
    service: RpcService,
    request: ListObjectsRequest,
) -> Result<BoxStream<'static, Result<ListObjectsResponse>>> {
    service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let filter = request
        .filter
        .as_ref()
        .map(owned_object_filter)
        .transpose()?
        .map(Arc::new);

    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        Arc::new(FieldMaskTree::from(read_mask))
    };

    let should_load = should_load_object_by_type(&read_mask);

    let opts = parse_query_options(
        request.options.as_ref(),
        request.at_checkpoint,
        DEFAULT_PAGE_SIZE,
        MAX_PAGE_SIZE,
        |field, key| match key {
            StateCursorKey::Objects { key } => Ok(key),
            _ => Err(wrong_cursor_scope(
                field,
                "cursor does not match ListObjects",
            )),
        },
    )?;

    // Acquire the snapshot handle and verify the object pruning floor on a blocking thread before
    // opening the stream so pre-stream refusals return directly as gRPC status errors.
    let pinned = {
        let service = service.clone();
        let checkpoint = opts.checkpoint;
        let pin_source = opts.pin_source;
        tokio::task::spawn_blocking(move || {
            resolve_pinned(service.reader.inner().as_ref(), checkpoint, pin_source, true)
        })
        .await
        .map_err(|e| RpcError::new(tonic::Code::Internal, format!("list_objects task failed: {e}")))??
    };
    let pinned = Arc::new(pinned);

    let (mut lower, mut upper) = opts.bounds();
    let descending = opts.descending;
    let exhausted_end_reason = opts.exhausted_end_reason();
    let mut remaining = opts.limit;

    let stream = async_stream::try_stream! {
        loop {
            let chunk_limit = remaining.min(CHUNK_SIZE);
            let chunk_service = service.clone();
            let chunk_pinned = pinned.clone();
            let chunk_filter = filter.clone();
            let chunk_mask = read_mask.clone();
            let chunk_lower = lower.clone();
            let chunk_upper = upper.clone();

            let (items, last_key) = tokio::task::spawn_blocking(move || {
                read_chunk(
                    &chunk_service,
                    &chunk_pinned,
                    chunk_filter.as_deref(),
                    &chunk_mask,
                    should_load,
                    chunk_lower,
                    chunk_upper,
                    descending,
                    chunk_limit,
                )
            })
            .await
            .map_err(|e| {
                RpcError::new(tonic::Code::Internal, format!("list_objects task failed: {e}"))
            })??;

            let count = items.len();
            if count == remaining {
                let mut iter = items.into_iter().peekable();
                while let Some((object, cursor)) = iter.next() {
                    let mut frame = ListObjectsResponse::default()
                        .with_object(object)
                        .with_cursor(cursor);
                    if iter.peek().is_none() {
                        frame.end = Some(
                            QueryEnd::default().with_reason(QueryEndReason::ItemLimit),
                        );
                    }
                    yield frame;
                }
                break;
            }

            for (object, cursor) in items {
                yield ListObjectsResponse::default()
                    .with_object(object)
                    .with_cursor(cursor);
            }

            if count < chunk_limit {
                yield ListObjectsResponse::default()
                    .with_end(QueryEnd::default().with_reason(exhausted_end_reason));
                break;
            }

            remaining -= count;
            let resume_key = last_key.expect("non-empty chunk always records last_key");
            if descending {
                upper = Bound::Excluded(resume_key);
            } else {
                lower = Bound::Excluded(resume_key);
            }
        }
    };

    Ok(stream.boxed())
}

#[allow(clippy::too_many_arguments)]
fn read_chunk(
    service: &RpcService,
    pinned: &PinnedStateRead,
    filter: Option<&OwnedObjectFilter>,
    read_mask: &FieldMaskTree,
    should_load: bool,
    lower: Bound<ObjectByTypeKey>,
    upper: Bound<ObjectByTypeKey>,
    descending: bool,
    chunk_limit: usize,
) -> Result<(Vec<(Object, Bytes)>, Option<ObjectByTypeKey>)> {
    let reader = service.reader.inner();
    let render_context = RenderContext::empty(reader);
    let mut iter = pinned
        .indexes
        .objects_by_type_iter(filter, lower, upper, descending)?;
    let mut items = Vec::with_capacity(chunk_limit);
    let mut last_key = None;

    while items.len() < chunk_limit {
        let Some(info) = iter
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        else {
            break;
        };

        let object = if should_load {
            let Some(object) = reader.get_object_by_key(&info.object_id, info.version) else {
                return Err(pinned.missing_object_error(
                    reader.as_ref(),
                    info.object_id,
                    info.version,
                ));
            };
            service.render_object_to_proto(&object, read_mask, &render_context)
        } else {
            object_by_type_to_proto(&info, read_mask)
        };

        let key = ObjectByTypeKey {
            object_type: info.object_type,
            object_id: info.object_id,
        };
        let cursor = StateCursor {
            checkpoint: pinned.checkpoint,
            key: StateCursorKey::Objects { key: key.clone() },
        }
        .encode();
        last_key = Some(key);
        items.push((object, cursor));
    }

    // Re-check the object pruning floor before releasing any frame from this chunk.
    pinned.check_object_floor(reader.as_ref())?;

    Ok((items, last_key))
}

fn should_load_object_by_type(mask: &FieldMaskTree) -> bool {
    // `object_by_type` stores only `(object_type, object_id) -> version`, not coin balance.
    mask.contains(Object::BALANCE_FIELD) || should_load_object(mask)
}

fn object_by_type_to_proto(info: &ObjectByTypeInfo, mask: &FieldMaskTree) -> Object {
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

    message
}
