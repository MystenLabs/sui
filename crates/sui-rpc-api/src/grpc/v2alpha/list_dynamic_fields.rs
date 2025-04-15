// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2alpha::dynamic_field::DynamicFieldKind;
use crate::proto::rpc::v2alpha::DynamicField;
use crate::proto::rpc::v2alpha::ListDynamicFieldsRequest;
use crate::proto::rpc::v2alpha::ListDynamicFieldsResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use sui_sdk_types::ObjectId;
use sui_types::storage::{DynamicFieldIndexInfo, DynamicFieldKey};
use tap::Pipe;

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

    let parent: ObjectId = request
        .parent
        .as_ref()
        .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "missing parent"))?
        .parse()
        .map_err(|e| RpcError::new(tonic::Code::InvalidArgument, format!("invalid parent: {e}")))?;

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 1000))
        .unwrap_or(50);
    let page_token = request
        .page_token
        .map(|token| decode_page_token(&token))
        .transpose()?;

    let mut dynamic_fields = indexes
        .dynamic_field_iter(parent.into(), page_token.map(Into::into))?
        .take(page_size + 1)
        .map(|result| {
            result
                .map(|(key, value)| convert_into_proto(key, value))
                .map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let next_page_token = if dynamic_fields.len() > page_size {
        // SAFETY: We've already verified that object_keys is greater than limit, which is
        // gaurenteed to be >= 1.
        dynamic_fields
            .pop()
            .unwrap()
            .field_id
            .unwrap()
            .parse::<ObjectId>()
            .unwrap()
            .pipe(encode_page_token)
            .pipe(Some)
    } else {
        None
    };

    Ok(ListDynamicFieldsResponse {
        dynamic_fields,
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<ObjectId> {
    Ok(ObjectId::new(page_token.try_into().unwrap()))
}

fn encode_page_token(page_token: ObjectId) -> Bytes {
    page_token.as_bytes().to_vec().into()
}

fn convert_into_proto(
    DynamicFieldKey { parent, field_id }: DynamicFieldKey,
    DynamicFieldIndexInfo {
        dynamic_field_kind,
        name_type,
        name_value,
        value_type,
        dynamic_object_id,
    }: DynamicFieldIndexInfo,
) -> DynamicField {
    let kind = match dynamic_field_kind {
        sui_types::dynamic_field::DynamicFieldType::DynamicField => DynamicFieldKind::Field,
        sui_types::dynamic_field::DynamicFieldType::DynamicObject => DynamicFieldKind::Object,
    };

    DynamicField {
        kind: Some(kind as i32),
        parent: Some(parent.to_canonical_string(true)),
        field_id: Some(field_id.to_canonical_string(true)),
        name_type: Some(name_type.to_canonical_string(true)),
        name_value: Some(name_value.into()),
        value_type: Some(value_type.to_canonical_string(true)),
        dynamic_object_id: dynamic_object_id.map(|id| id.to_canonical_string(true)),
    }
}
