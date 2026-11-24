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
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::DynamicField;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsResponse;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::dynamic_field::DynamicFieldKind;
use sui_sdk_types::Address;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use sui_types::storage::DynamicFieldEntry;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::OwnedObjectOwner;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::render::RenderContext;
use crate::reader::indexes_disabled;

use super::checkpoint::PinSource;
use super::checkpoint::PinnedStateRead;
use super::checkpoint::resolve_pinned;
use super::checkpoint::resolve_state_read;
use super::cursor::DecodedPageToken;
use super::cursor::StateCursor;
use super::cursor::StateCursorKey;
use super::cursor::decode_page_token;
use super::cursor::parse_query_options;
use super::cursor::reject_legacy_paging_with_options;
use super::cursor::wrong_cursor_scope;
use super::filter::dynamic_field_filter;

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::DYNAMIC_FIELD;

#[tracing::instrument(skip(service))]
pub fn list_dynamic_fields(
    service: &RpcService,
    request: ListDynamicFieldsRequest,
) -> Result<ListDynamicFieldsResponse> {
    let reader = service.reader.inner();
    reader.indexes().ok_or_else(indexes_disabled)?;

    let parent: Address = request
        .parent
        .as_ref()
        .ok_or_else(|| {
            FieldViolation::new(ListDynamicFieldsRequest::PARENT_FIELD.name)
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse()
        .map_err(|e| {
            FieldViolation::new(ListDynamicFieldsRequest::PARENT_FIELD.name)
                .with_description(format!("invalid owner: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    let parent_id = ObjectID::from(parent);

    let filter = request
        .filter
        .as_ref()
        .map(dynamic_field_filter)
        .transpose()?;

    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<DynamicField>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let extract_key = |field: &str, key: StateCursorKey| match key {
        StateCursorKey::DynamicFields {
            parent: cur_parent,
            key,
        } if cur_parent == parent_id => Ok(key),
        _ => Err(wrong_cursor_scope(
            field,
            "cursor does not match this ListDynamicFields parent",
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
        let mut iter = pinned.indexes.dynamic_field_iter(
            parent_id,
            filter.as_ref(),
            lower,
            upper,
            opts.descending,
        )?;
        let mut dynamic_fields = Vec::with_capacity(opts.limit);
        let mut cursors = Vec::with_capacity(opts.limit);
        let render_context = RenderContext::empty(reader);
        let mut size_bytes = 0;
        let mut hit_limit = false;

        while let Some(entry) = iter
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        {
            let dynamic_field =
                get_dynamic_field_pinned(service, &pinned, &entry, &read_mask, &render_context)?;
            let cursor = StateCursor {
                checkpoint: pinned.checkpoint,
                key: StateCursorKey::DynamicFields {
                    parent: parent_id,
                    key: entry.key(),
                },
            }
            .encode();

            size_bytes += dynamic_field.encoded_len();
            dynamic_fields.push(dynamic_field);
            cursors.push(cursor);

            if dynamic_fields.len() >= opts.limit || size_bytes >= MAX_PAGE_SIZE_BYTES {
                hit_limit = true;
                break;
            }
        }

        pinned.check_object_floor(reader.as_ref())?;

        let end_reason = if hit_limit {
            QueryEndReason::ItemLimit
        } else {
            opts.exhausted_end_reason()
        };
        let mut message = ListDynamicFieldsResponse::default();
        message.dynamic_fields = dynamic_fields;
        message.cursors = cursors;
        message.end = Some(QueryEnd::default().with_reason(end_reason));
        return Ok(message);
    }

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let page_token = request
        .page_token
        .as_deref()
        .map(|token| {
            decode_page_token::<LegacyPageToken, DynamicFieldKey, _, _>(
                token,
                request.at_checkpoint,
                |t| t.parent == parent,
                extract_key,
            )
        })
        .transpose()?;

    let (checkpoint, pin, pin_source, lower) = match page_token {
        Some(DecodedPageToken::Legacy(token)) => (
            None,
            false,
            PinSource::AtCheckpoint,
            Bound::Included(DynamicFieldKey {
                parent: parent_id,
                field_id: token.field_id.into(),
                object_type: token.object_type,
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

    let mut iter = target.indexes().dynamic_field_iter(
        parent_id,
        filter.as_ref(),
        lower,
        Bound::Unbounded,
        false,
    )?;
    let mut dynamic_fields = Vec::with_capacity(page_size);
    let render_context = RenderContext::empty(reader);
    let mut size_bytes = 0;
    let mut last_key: Option<DynamicFieldKey> = None;

    while let Some(entry) = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    {
        let dynamic_field = if let Some(pinned) = target.pinned() {
            get_dynamic_field_pinned(service, pinned, &entry, &read_mask, &render_context)?
        } else {
            let Some(dynamic_field) = get_dynamic_field_current(
                service,
                &entry.parent,
                &entry.field_id,
                &read_mask,
                &render_context,
            ) else {
                continue;
            };
            dynamic_field
        };

        last_key = Some(entry.key());
        size_bytes += dynamic_field.encoded_len();
        dynamic_fields.push(dynamic_field);

        if dynamic_fields.len() >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }
    }

    let next_entry = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    if let Some(pinned) = target.pinned() {
        pinned.check_object_floor(reader.as_ref())?;
    }

    let next_page_token = next_entry.map(|cursor| {
        if let Some(pinned) = target.pinned() {
            StateCursor {
                checkpoint: pinned.checkpoint,
                key: StateCursorKey::DynamicFields {
                    parent: parent_id,
                    key: last_key.expect("next row only peeked after non-empty page"),
                },
            }
            .encode()
        } else {
            encode_legacy_page_token(LegacyPageToken {
                parent,
                field_id: cursor.field_id.into(),
                object_type: cursor.object_type,
            })
        }
    });

    let mut message = ListDynamicFieldsResponse::default();
    message.dynamic_fields = dynamic_fields;
    message.next_page_token = next_page_token;
    Ok(message)
}

fn encode_legacy_page_token(page_token: LegacyPageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

/// Does not record `filter`: resuming with a different filter continues from
/// the same index position.
#[derive(serde::Serialize, serde::Deserialize)]
struct LegacyPageToken {
    parent: Address,
    field_id: Address,
    object_type: StructTag,
}

fn get_dynamic_field_pinned(
    service: &RpcService,
    pinned: &PinnedStateRead,
    entry: &DynamicFieldEntry,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Result<DynamicField> {
    let mut message = DynamicField::default();

    if read_mask.contains(DynamicField::PARENT_FIELD) {
        message.parent = Some(entry.parent.to_canonical_string(true));
    }

    if read_mask.contains(DynamicField::FIELD_ID_FIELD) {
        message.field_id = Some(entry.field_id.to_canonical_string(true));
    }

    if should_load_field(read_mask) {
        let reader = service.reader.inner();
        let field_object = reader
            .get_object_by_key(&entry.field_id, entry.version)
            .ok_or_else(|| {
                pinned.missing_object_error(reader.as_ref(), entry.field_id, entry.version)
            })?;
        load_dynamic_field_pinned(
            service,
            pinned,
            &field_object,
            read_mask,
            &mut message,
            render_context,
        )?;
    }

    Ok(message)
}

fn get_dynamic_field_current(
    service: &RpcService,
    parent: &ObjectID,
    field_id: &ObjectID,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Option<DynamicField> {
    let mut message = DynamicField::default();

    if read_mask.contains(DynamicField::PARENT_FIELD) {
        message.parent = Some(parent.to_canonical_string(true));
    }

    if read_mask.contains(DynamicField::FIELD_ID_FIELD) {
        message.field_id = Some(field_id.to_canonical_string(true));
    }

    if should_load_field(read_mask) {
        // The index applies a checkpoint only after its transactions have
        // executed, so for a moment it can still list a field whose object
        // has been deleted. That field no longer exists; skip it rather than
        // return an entry with nothing but its ids.
        let field_object = service.reader.inner().get_object(field_id)?;
        if let Err(e) = load_dynamic_field_current(
            service,
            &field_object,
            read_mask,
            &mut message,
            render_context,
        ) {
            tracing::warn!("error loading dynamic object: {e}");
            return None;
        }
    }

    Some(message)
}

fn should_load_field(mask: &FieldMaskTree) -> bool {
    [
        DynamicField::KIND_FIELD,
        DynamicField::NAME_FIELD,
        DynamicField::VALUE_FIELD,
        DynamicField::VALUE_TYPE_FIELD,
        DynamicField::CHILD_ID_FIELD,
        DynamicField::CHILD_OBJECT_FIELD,
        DynamicField::FIELD_OBJECT_FIELD,
    ]
    .into_iter()
    .any(|field| mask.contains(field))
}

fn load_dynamic_field_pinned(
    service: &RpcService,
    pinned: &PinnedStateRead,
    field_object: &Object,
    read_mask: &FieldMaskTree,
    message: &mut DynamicField,
    render_context: &RenderContext,
) -> Result<()> {
    let Some(child_id) = populate_field_object(
        service,
        field_object,
        read_mask,
        message,
        render_context,
    )
    .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    else {
        return Ok(());
    };

    if read_mask.contains(DynamicField::VALUE_TYPE_FIELD)
        || read_mask.contains(DynamicField::CHILD_OBJECT_FIELD)
    {
        let field_id = field_object.id();
        // Resolve the child object's version at the pinned checkpoint by scanning
        // `object_by_owner` under the field object as owner through the same snapshot.
        let child_info = pinned
            .indexes
            .owned_objects_iter(
                OwnedObjectOwner::Object(field_id),
                None,
                Bound::Unbounded,
                Bound::Unbounded,
                false,
            )?
            .find_map(|res| match res {
                Ok(info) if info.object_id == child_id => Some(Ok(info)),
                Ok(_) => None,
                Err(e) => Some(Err(RpcError::new(tonic::Code::Internal, e.to_string()))),
            })
            .transpose()?
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!(
                        "missing dynamic object field child {child_id} under field {field_id} at \
                         checkpoint {}",
                        pinned.checkpoint
                    ),
                )
            })?;

        let reader = service.reader.inner();
        let child_object = reader
            .get_object_by_key(&child_id, child_info.version)
            .ok_or_else(|| {
                pinned.missing_object_error(reader.as_ref(), child_id, child_info.version)
            })?;
        populate_child_object(service, &child_object, read_mask, message, render_context)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    }

    Ok(())
}

fn load_dynamic_field_current(
    service: &RpcService,
    field_object: &Object,
    read_mask: &FieldMaskTree,
    message: &mut DynamicField,
    render_context: &RenderContext,
) -> Result<(), anyhow::Error> {
    let Some(child_id) =
        populate_field_object(service, field_object, read_mask, message, render_context)?
    else {
        return Ok(());
    };

    if read_mask.contains(DynamicField::VALUE_TYPE_FIELD)
        || read_mask.contains(DynamicField::CHILD_OBJECT_FIELD)
    {
        let child_object = service
            .reader
            .inner()
            .get_object(&child_id)
            .ok_or_else(|| anyhow::anyhow!("missing dynamic object {child_id}"))?;
        populate_child_object(service, &child_object, read_mask, message, render_context)?;
    }

    Ok(())
}

/// Populate `message` from `field_object` and return the child `ObjectID` when this is a dynamic
/// object field.
fn populate_field_object(
    service: &RpcService,
    field_object: &Object,
    read_mask: &FieldMaskTree,
    message: &mut DynamicField,
    render_context: &RenderContext,
) -> Result<Option<ObjectID>, anyhow::Error> {
    use sui_types::dynamic_field::DynamicFieldType;
    use sui_types::dynamic_field::visitor as DFV;

    // Skip if not a move object
    let Some(move_object) = field_object.data.try_as_move() else {
        return Ok(None);
    };

    // Skip any objects that aren't of type `Field<Name, Value>`
    //
    // All dynamic fields are of type:
    //   - Field<Name, Value> for dynamic fields
    //   - Field<Wrapper<Name>, ID>> for dynamic field objects where the ID is the id of the pointed
    //   to object
    if !move_object.type_().is_dynamic_field() {
        return Ok(None);
    }

    let layout = render_context
        .type_layout(move_object.type_().clone().into())
        .map_err(|e| {
            anyhow::anyhow!(
                "unable to load layout for type `{:?}`: {e}",
                move_object.type_()
            )
        })?;

    let field = DFV::FieldVisitor::deserialize(move_object.contents(), &layout)?;

    if read_mask.contains(DynamicField::KIND_FIELD) {
        let kind = match field.kind {
            DynamicFieldType::DynamicField => DynamicFieldKind::Field,
            DynamicFieldType::DynamicObject => DynamicFieldKind::Object,
        };
        message.set_kind(kind);
    }

    if read_mask.contains(DynamicField::NAME_FIELD) {
        message.name = Some(
            Bcs::default()
                .with_name(sui_types::TypeTag::from(field.name_layout).to_canonical_string(true))
                .with_value(field.name_bytes.to_vec()),
        );
    }

    if read_mask.contains(DynamicField::VALUE_FIELD) {
        message.value = Some(
            Bcs::default()
                .with_name(sui_types::TypeTag::from(field.value_layout).to_canonical_string(true))
                .with_value(field.value_bytes.to_vec()),
        );
    }

    if let Some(submask) = read_mask.subtree(DynamicField::FIELD_OBJECT_FIELD) {
        message.set_field_object(service.render_object_to_proto(
            field_object,
            &submask,
            render_context,
        ));
    }

    match field.value_metadata()? {
        DFV::ValueMetadata::DynamicField(type_tag) => {
            if read_mask.contains(DynamicField::VALUE_TYPE_FIELD) {
                message.value_type = Some(type_tag.to_canonical_string(true));
            }
            Ok(None)
        }
        DFV::ValueMetadata::DynamicObjectField(object_id) => {
            if read_mask.contains(DynamicField::CHILD_ID_FIELD) {
                message.child_id = Some(object_id.to_canonical_string(true));
            }
            Ok(Some(object_id))
        }
    }
}

fn populate_child_object(
    service: &RpcService,
    child_object: &Object,
    read_mask: &FieldMaskTree,
    message: &mut DynamicField,
    render_context: &RenderContext,
) -> Result<(), anyhow::Error> {
    let type_tag = sui_types::TypeTag::from(
        child_object
            .struct_tag()
            .ok_or_else(|| anyhow::anyhow!("dynamic object field cannot be a package"))?,
    );
    if read_mask.contains(DynamicField::VALUE_TYPE_FIELD) {
        message.value_type = Some(type_tag.to_canonical_string(true));
    }

    if let Some(submask) = read_mask.subtree(DynamicField::CHILD_OBJECT_FIELD) {
        message.set_child_object(service.render_object_to_proto(
            child_object,
            &submask,
            render_context,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_types::base_types::SequenceNumber;
    use sui_types::object::Owner;
    use sui_types::storage::OwnedObjectKey;

    use super::*;
    use crate::reader::assert_refusal;
    use crate::grpc::v2::state_service::test_support::TestStateStore;
    use crate::grpc::v2::state_service::test_support::dummy_struct_bytes;
    use crate::grpc::v2::state_service::test_support::make_dynamic_field_u64;
    use crate::grpc::v2::state_service::test_support::make_dynamic_object_field_u64;
    use crate::grpc::v2::state_service::test_support::make_struct_object;
    use crate::grpc::v2::state_service::test_support::struct_tag;
    use crate::grpc::v2::state_service::test_support::test_object_id;

    #[test]
    fn pinned_read_loads_exact_field_and_child_versions_at_checkpoint() {
        let (_dir, store, service) = TestStateStore::open(5);
        let parent = test_object_id(0xAA);
        let inline_field_id = test_object_id(0x01);
        let dof_field_id = test_object_id(0x02);
        let child_id = test_object_id(0x03);
        let child_tag = struct_tag("0x9::item::Widget");

        // Checkpoint 10: inline field (v1, value=111) + dynamic object field (v1) owning child (v1, payload=222).
        let inline_v1 = make_dynamic_field_u64(parent, inline_field_id, 1, 7, 111);
        let dof_v1 = make_dynamic_object_field_u64(parent, dof_field_id, 1, 8, child_id);
        let child_v1 = make_struct_object(
            child_id,
            1,
            Owner::ObjectOwner(dof_field_id.into()),
            child_tag.clone(),
            dummy_struct_bytes(child_id, 222),
        );
        store.put_object(inline_v1.clone());
        store.put_object(dof_v1.clone());
        store.put_object(child_v1.clone());
        store.take_snapshot(10);

        // Checkpoint 11: mutate inline field (v2, value=999), dof field (v2), AND child (v2, payload=888).
        let inline_v2 = make_dynamic_field_u64(parent, inline_field_id, 2, 7, 999);
        let dof_v2 = make_dynamic_object_field_u64(parent, dof_field_id, 2, 8, child_id);
        let child_v2 = make_struct_object(
            child_id,
            2,
            Owner::ObjectOwner(dof_field_id.into()),
            child_tag,
            dummy_struct_bytes(child_id, 888),
        );
        store.replace_object(&inline_v1, inline_v2);
        store.replace_object(&dof_v1, dof_v2);
        store.replace_object(&child_v1, child_v2);
        store.take_snapshot(11);

        let mask = FieldMask::from_str(
            "kind,parent,field_id,name,value,value_type,child_id,field_object.version,child_object.version,child_object.contents",
        );

        // Read at checkpoint 10: MUST return v1 for both fields AND v1 (payload 222) for the child!
        let mut req10 = ListDynamicFieldsRequest::default();
        req10.parent = Some(parent.to_string());
        req10.at_checkpoint = Some(10);
        req10.read_mask = Some(mask.clone());
        req10.options = Some(QueryOptions::default());
        let resp10 = list_dynamic_fields(&service, req10).unwrap();
        assert_eq!(resp10.dynamic_fields.len(), 2);
        assert_eq!(resp10.cursors.len(), 2);
        assert_eq!(resp10.end.unwrap().reason(), QueryEndReason::CheckpointBound);

        let inline_10 = resp10
            .dynamic_fields
            .iter()
            .find(|f| f.kind() == DynamicFieldKind::Field)
            .unwrap();
        assert_eq!(inline_10.field_object.as_ref().unwrap().version(), 1);
        assert_eq!(
            inline_10.value.as_ref().unwrap().value(),
            111u64.to_le_bytes().as_slice()
        );

        let dof_10 = resp10
            .dynamic_fields
            .iter()
            .find(|f| f.kind() == DynamicFieldKind::Object)
            .unwrap();
        assert_eq!(dof_10.field_object.as_ref().unwrap().version(), 1);
        assert_eq!(dof_10.child_object.as_ref().unwrap().version(), 1);
        assert_eq!(
            dof_10
                .child_object
                .as_ref()
                .unwrap()
                .contents
                .as_ref()
                .unwrap()
                .value(),
            dummy_struct_bytes(child_id, 222).as_slice()
        );

        // Read at checkpoint 11: returns v2 for both fields and child (payload 888).
        let mut req11 = ListDynamicFieldsRequest::default();
        req11.parent = Some(parent.to_string());
        req11.at_checkpoint = Some(11);
        req11.read_mask = Some(mask);
        req11.options = Some(QueryOptions::default());
        let resp11 = list_dynamic_fields(&service, req11).unwrap();
        let inline_11 = resp11
            .dynamic_fields
            .iter()
            .find(|f| f.kind() == DynamicFieldKind::Field)
            .unwrap();
        assert_eq!(inline_11.field_object.as_ref().unwrap().version(), 2);
        assert_eq!(
            inline_11.value.as_ref().unwrap().value(),
            999u64.to_le_bytes().as_slice()
        );
        let dof_11 = resp11
            .dynamic_fields
            .iter()
            .find(|f| f.kind() == DynamicFieldKind::Object)
            .unwrap();
        assert_eq!(dof_11.field_object.as_ref().unwrap().version(), 2);
        assert_eq!(dof_11.child_object.as_ref().unwrap().version(), 2);
        assert_eq!(
            dof_11
                .child_object
                .as_ref()
                .unwrap()
                .contents
                .as_ref()
                .unwrap()
                .value(),
            dummy_struct_bytes(child_id, 888).as_slice()
        );

        // Descending order reverses the two dynamic fields.
        let mut desc_opts = QueryOptions::default();
        desc_opts.ordering = Some(Ordering::Descending as i32);
        let mut req_desc = ListDynamicFieldsRequest::default();
        req_desc.parent = Some(parent.to_string());
        req_desc.at_checkpoint = Some(10);
        req_desc.options = Some(desc_opts);
        let resp_desc = list_dynamic_fields(&service, req_desc).unwrap();
        assert_eq!(resp_desc.dynamic_fields[0].field_id(), resp10.dynamic_fields[1].field_id());
        assert_eq!(resp_desc.dynamic_fields[1].field_id(), resp10.dynamic_fields[0].field_id());

        // Reject a ListOwnedObjects(OBJECT, parent) cursor passed to ListDynamicFields.
        let owned_cursor = StateCursor {
            checkpoint: 10,
            key: StateCursorKey::OwnedObjects {
                owner: OwnedObjectOwner::Object(parent),
                key: OwnedObjectKey {
                    object_type: struct_tag("0x2::dynamic_field::Field<u64, u64>"),
                    balance: None,
                    object_id: inline_field_id,
                },
            },
        }
        .encode();
        let mut bad_opts = QueryOptions::default();
        bad_opts.after = Some(owned_cursor);
        let mut bad_req = ListDynamicFieldsRequest::default();
        bad_req.parent = Some(parent.to_string());
        bad_req.options = Some(bad_opts);
        assert_eq!(
            list_dynamic_fields(&service, bad_req).unwrap_err().code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn pinned_refuses_missing_field_or_child_while_unpinned_skips() {
        let (_dir, store, service) = TestStateStore::open(5);
        let parent = test_object_id(0xAA);
        let dof_field_id = test_object_id(0x02);
        let child_id = test_object_id(0x03);

        let dof_v1 = make_dynamic_object_field_u64(parent, dof_field_id, 1, 8, child_id);
        let child_v1 = make_struct_object(
            child_id,
            1,
            Owner::ObjectOwner(dof_field_id.into()),
            struct_tag("0x9::item::Widget"),
            dummy_struct_bytes(child_id, 42),
        );
        store.put_object(dof_v1);
        store.put_object(child_v1);
        store.take_snapshot(1);

        // Drop child version -> pinned read requesting child_object refuses with pruning_race.
        store.drop_object_version(child_id, SequenceNumber::from_u64(1));
        let mut pinned_req = ListDynamicFieldsRequest::default();
        pinned_req.parent = Some(parent.to_string());
        pinned_req.at_checkpoint = Some(1);
        pinned_req.read_mask = Some(FieldMask::from_str("kind,child_object.version"));
        assert_refusal(
            list_dynamic_fields(&service, pinned_req.clone()).unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );

        // Drop field version too -> pinned refuses with pruning_race, unpinned skips cleanly.
        store.drop_object_version(dof_field_id, SequenceNumber::from_u64(1));
        assert_refusal(
            list_dynamic_fields(&service, pinned_req).unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "pruning_race",
        );

        let mut unpinned_req = ListDynamicFieldsRequest::default();
        unpinned_req.parent = Some(parent.to_string());
        unpinned_req.read_mask = Some(FieldMask::from_str("kind,field_object.version"));
        let unpinned = list_dynamic_fields(&service, unpinned_req).unwrap();
        assert!(unpinned.dynamic_fields.is_empty());
    }
}
