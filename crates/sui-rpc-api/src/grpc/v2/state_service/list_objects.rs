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

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ObjectFilter;
    use sui_rpc::proto::sui::rpc::v2::ObjectLiteral;
    use sui_rpc::proto::sui::rpc::v2::ObjectTerm;
    use sui_rpc::proto::sui::rpc::v2::ObjectTypeFilter;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc::proto::sui::rpc::v2::object_literal;
    use sui_types::base_types::SequenceNumber;
    use sui_types::object::Owner;
    use sui_types::storage::OwnedObjectKey;
    use sui_types::storage::OwnedObjectOwner;

    use super::*;
    use crate::reader::assert_refusal;
    use crate::grpc::v2::state_service::test_support::TestStateStore;
    use crate::grpc::v2::state_service::test_support::dummy_struct_bytes;
    use crate::grpc::v2::state_service::test_support::make_struct_object;
    use crate::grpc::v2::state_service::test_support::struct_tag;
    use crate::grpc::v2::state_service::test_support::test_address;
    use crate::grpc::v2::state_service::test_support::test_object_id;

    fn type_filter(object_type: &str, negated: bool) -> ObjectFilter {
        let mut tf = ObjectTypeFilter::default();
        tf.object_type = Some(object_type.to_owned());
        let mut lit = ObjectLiteral::default();
        lit.negated = negated;
        lit.predicate = Some(object_literal::Predicate::ObjectType(tf));
        let mut term = ObjectTerm::default();
        term.literals = vec![lit];
        let mut filter = ObjectFilter::default();
        filter.terms = vec![term];
        filter
    }

    async fn collect_frames(
        service: &RpcService,
        request: ListObjectsRequest,
    ) -> Result<Vec<ListObjectsResponse>> {
        let stream = list_objects(service.clone(), request).await?;
        stream.try_collect().await
    }

    #[tokio::test]
    async fn streams_unfiltered_filtered_reverse_and_exact_frame_invariants() {
        let (_dir, store, service) = TestStateStore::open(5);
        let alice = test_address(1);
        let o1 = make_struct_object(
            test_object_id(1),
            1,
            Owner::AddressOwner(alice),
            struct_tag("0x1::a::A"),
            dummy_struct_bytes(test_object_id(1), 1),
        );
        let o2 = make_struct_object(
            test_object_id(2),
            1,
            Owner::Shared {
                initial_shared_version: SequenceNumber::from_u64(1),
            },
            struct_tag("0x2::b::B"),
            dummy_struct_bytes(test_object_id(2), 2),
        );
        let o3 = make_struct_object(
            test_object_id(3),
            1,
            Owner::Immutable,
            struct_tag("0x3::c::C"),
            dummy_struct_bytes(test_object_id(3), 3),
        );
        store.put_object(o1.clone());
        store.put_object(o2);
        store.put_object(o3);
        store.take_snapshot(10);

        // Mutate o1 at checkpoint 11; checkpoint 10 stream must still see v1.
        let o1_v2 = make_struct_object(
            test_object_id(1),
            2,
            Owner::AddressOwner(alice),
            struct_tag("0x1::a::A"),
            dummy_struct_bytes(test_object_id(1), 99),
        );
        store.replace_object(&o1, o1_v2);
        store.take_snapshot(11);

        // Unfiltered ascending at checkpoint 10: 3 item frames + 1 empty terminal CHECKPOINT_BOUND frame.
        let mut req_all = ListObjectsRequest::default();
        req_all.at_checkpoint = Some(10);
        req_all.read_mask = Some(FieldMask::from_str("object_id,version,contents"));
        let frames = collect_frames(&service, req_all).await.unwrap();
        assert_eq!(frames.len(), 4);
        for f in &frames[..3] {
            assert!(f.object.is_some());
            assert!(f.cursor.is_some());
            assert!(f.end.is_none());
        }
        assert_eq!(frames[0].object.as_ref().unwrap().version(), 1);
        assert!(frames[3].object.is_none());
        assert!(frames[3].cursor.is_none());
        assert_eq!(
            frames[3].end.as_ref().unwrap().reason(),
            QueryEndReason::CheckpointBound
        );

        // ITEM_LIMIT rides on the last item's frame with no trailing empty frame.
        let mut lim_opts = QueryOptions::default();
        lim_opts.limit = Some(2);
        let mut req_lim = ListObjectsRequest::default();
        req_lim.at_checkpoint = Some(10);
        req_lim.options = Some(lim_opts);
        let lim_frames = collect_frames(&service, req_lim).await.unwrap();
        assert_eq!(lim_frames.len(), 2);
        assert!(lim_frames[0].end.is_none());
        assert!(lim_frames[1].object.is_some());
        assert!(lim_frames[1].cursor.is_some());
        assert_eq!(
            lim_frames[1].end.as_ref().unwrap().reason(),
            QueryEndReason::ItemLimit
        );

        // Filtered negated (`!0x2::b::B`) descending returns [o3, o1] + empty CHECKPOINT_BOUND frame.
        let mut desc_opts = QueryOptions::default();
        desc_opts.ordering = Some(Ordering::Descending as i32);
        let mut req_filt = ListObjectsRequest::default();
        req_filt.at_checkpoint = Some(10);
        req_filt.filter = Some(type_filter("0x2::b::B", true));
        req_filt.options = Some(desc_opts);
        let filt_frames = collect_frames(&service, req_filt).await.unwrap();
        assert_eq!(filt_frames.len(), 3);
        assert_eq!(
            filt_frames[0].object.as_ref().unwrap().object_id(),
            test_object_id(3).to_canonical_string(true)
        );
        assert_eq!(
            filt_frames[1].object.as_ref().unwrap().object_id(),
            test_object_id(1).to_canonical_string(true)
        );
        assert!(filt_frames[2].object.is_none());
        assert_eq!(
            filt_frames[2].end.as_ref().unwrap().reason(),
            QueryEndReason::CheckpointBound
        );

        // Empty matching result yields a single empty frame with end set (CURSOR_BOUND when far cursor set).
        let mut bounded_opts = QueryOptions::default();
        bounded_opts.after = Some(frames[0].cursor.clone().unwrap());
        bounded_opts.before = Some(frames[1].cursor.clone().unwrap());
        let mut req_empty = ListObjectsRequest::default();
        req_empty.options = Some(bounded_opts);
        let empty_frames = collect_frames(&service, req_empty).await.unwrap();
        assert_eq!(empty_frames.len(), 1);
        assert!(empty_frames[0].object.is_none());
        assert!(empty_frames[0].cursor.is_none());
        assert_eq!(
            empty_frames[0].end.as_ref().unwrap().reason(),
            QueryEndReason::CursorBound
        );

        // Reject cursor from ListOwnedObjects.
        let owned_cursor = StateCursor {
            checkpoint: 10,
            key: StateCursorKey::OwnedObjects {
                owner: OwnedObjectOwner::Address(alice),
                key: OwnedObjectKey {
                    object_type: struct_tag("0x1::a::A"),
                    balance: None,
                    object_id: test_object_id(1),
                },
            },
        }
        .encode();
        let mut wrong_opts = QueryOptions::default();
        wrong_opts.after = Some(owned_cursor);
        let mut req_wrong = ListObjectsRequest::default();
        req_wrong.options = Some(wrong_opts);
        assert_eq!(
            list_objects(service.clone(), req_wrong)
                .await
                .err()
                .unwrap()
                .code(),
            tonic::Code::InvalidArgument
        );

        // Missing object version fails stream with pruning_race and does not emit QueryEnd.
        store.drop_object_version(test_object_id(1), SequenceNumber::from_u64(1));
        let mut req_race = ListObjectsRequest::default();
        req_race.at_checkpoint = Some(10);
        req_race.read_mask = Some(FieldMask::from_str("object_id,contents"));
        assert_refusal(
            collect_frames(&service, req_race).await.unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );
    }
}
