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
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::DynamicField;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsResponse;
use sui_rpc::proto::sui::rpc::v2::dynamic_field::DynamicFieldKind;
use sui_sdk_types::Address;
use sui_types::base_types::ObjectID;

const MAX_PAGE_SIZE: usize = 1000;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB
const READ_MASK_DEFAULT: &str = "parent,field_id";

#[tracing::instrument(skip(service))]
pub fn list_dynamic_fields(
    service: &RpcService,
    request: ListDynamicFieldsRequest,
) -> Result<ListDynamicFieldsResponse> {
    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(RpcError::not_found)?;

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

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;

    if let Some(token) = &page_token
        && token.parent != parent
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
        read_mask.validate::<DynamicField>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };

    let mut iter =
        indexes.dynamic_field_iter(parent.into(), page_token.map(|t| t.field_id.into()))?;
    let mut dynamic_fields = Vec::with_capacity(page_size);
    let mut size_bytes = 0;
    while let Some(key) = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    {
        let Some(dynamic_field) =
            get_dynamic_field(service, &key.parent, &key.field_id, &read_mask)
        else {
            continue;
        };

        size_bytes += dynamic_field.encoded_len();
        dynamic_fields.push(dynamic_field);

        if dynamic_fields.len() >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }
    }

    let next_page_token = iter
        .next()
        .transpose()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .map(|cursor| {
            encode_page_token(PageToken {
                parent,
                field_id: cursor.field_id.into(),
            })
        });

    let mut message = ListDynamicFieldsResponse::default();
    message.dynamic_fields = dynamic_fields;
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

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    parent: Address,
    field_id: Address,
}

fn get_dynamic_field(
    service: &RpcService,
    parent: &ObjectID,
    field_id: &ObjectID,
    read_mask: &FieldMaskTree,
) -> Option<DynamicField> {
    let mut message = DynamicField::default();

    if read_mask.contains(DynamicField::PARENT_FIELD) {
        message.parent = Some(parent.to_canonical_string(true));
    }

    if read_mask.contains(DynamicField::FIELD_ID_FIELD) {
        message.field_id = Some(field_id.to_canonical_string(true));
    }

    if should_load_field(read_mask)
        && let Err(e) = load_dynamic_field(service, field_id, read_mask, &mut message)
    {
        tracing::warn!("error loading dynamic object: {e}");
        return None;
    }

    Some(message)
}

fn should_load_field(mask: &FieldMaskTree) -> bool {
    [
        DynamicField::KIND_FIELD,
        DynamicField::NAME_FIELD,
        DynamicField::VALUE_TYPE_FIELD,
        DynamicField::CHILD_ID_FIELD,
        DynamicField::CHILD_OBJECT_FIELD,
        DynamicField::FIELD_OBJECT_FIELD,
    ]
    .into_iter()
    .any(|field| mask.contains(field))
}

fn load_dynamic_field(
    service: &RpcService,
    field_id: &ObjectID,
    read_mask: &FieldMaskTree,
    message: &mut DynamicField,
) -> Result<(), anyhow::Error> {
    use sui_types::dynamic_field::DynamicFieldType;
    use sui_types::dynamic_field::visitor as DFV;

    let Some(field_object) = service.reader.inner().get_object(field_id) else {
        return Ok(());
    };

    // Skip if not a move object
    let Some(move_object) = field_object.data.try_as_move() else {
        return Ok(());
    };

    // Skip any objects that aren't of type `Field<Name, Value>`
    //
    // All dynamic fields are of type:
    //   - Field<Name, Value> for dynamic fields
    //   - Field<Wrapper<Name>, ID>> for dynamic field objects where the ID is the id of the pointed
    //   to object
    //
    if !move_object.type_().is_dynamic_field() {
        return Ok(());
    }

    let layout = match service
        .reader
        .inner()
        .get_struct_layout(&move_object.type_().clone().into())
    {
        Ok(Some(layout)) => layout,
        Ok(None) => {
            return Err(anyhow::anyhow!(
                "unable to load layout for type `{:?}`",
                move_object.type_()
            ));
        }
        Err(e) => {
            return Err(anyhow::anyhow!(
                "unable to load layout for type `{:?}`: {e}",
                move_object.type_()
            ));
        }
    };

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
        message.set_field_object(service.render_object_to_proto(&field_object, &submask));
    }

    match field.value_metadata()? {
        DFV::ValueMetadata::DynamicField(type_tag) => {
            if read_mask.contains(DynamicField::VALUE_TYPE_FIELD) {
                message.value_type = Some(type_tag.to_canonical_string(true));
            }
        }
        DFV::ValueMetadata::DynamicObjectField(object_id) => {
            if read_mask.contains(DynamicField::CHILD_ID_FIELD) {
                message.child_id = Some(object_id.to_canonical_string(true));
            }

            if read_mask.contains(DynamicField::VALUE_TYPE_FIELD)
                || read_mask.contains(DynamicField::CHILD_OBJECT_FIELD)
            {
                let object = service
                    .reader
                    .inner()
                    .get_object(&object_id)
                    .ok_or_else(|| anyhow::anyhow!("missing dynamic object {object_id}"))?;
                let type_tag =
                    sui_types::TypeTag::from(object.struct_tag().ok_or_else(|| {
                        anyhow::anyhow!("dynamic object field cannot be a package")
                    })?);
                if read_mask.contains(DynamicField::VALUE_TYPE_FIELD) {
                    message.value_type = Some(type_tag.to_canonical_string(true));
                }

                if let Some(submask) = read_mask.subtree(DynamicField::CHILD_OBJECT_FIELD) {
                    message.set_child_object(service.render_object_to_proto(&object, &submask));
                }
            }
        }
    };

    Ok(())
}
