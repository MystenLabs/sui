// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::rpc::v2alpha::DynamicField;
use crate::proto::rpc::v2alpha::ListDynamicFieldsRequest;
use crate::proto::rpc::v2alpha::ListDynamicFieldsResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use sui_sdk_types::ObjectId;
use sui_sdk_types::TypeTag;
use sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk;
use sui_types::{
    storage::{DynamicFieldIndexInfo, DynamicFieldKey},
    sui_sdk_types_conversions::SdkTypeConversionError,
};
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
                .map_err(|err| RpcError::new(tonic::Code::Internal, err.to_string()))
                .and_then(|x| DynamicFieldInfo::try_from(x)?.pipe(Ok))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let next_page_token = if dynamic_fields.len() > page_size {
        // SAFETY: We've already verified that object_keys is greater than limit, which is
        // gaurenteed to be >= 1.
        dynamic_fields
            .pop()
            .unwrap()
            .field_id
            .pipe(ObjectId::from)
            .pipe(encode_page_token)
            .pipe(Some)
    } else {
        None
    };

    Ok(ListDynamicFieldsResponse {
        dynamic_fields: dynamic_fields
            .into_iter()
            .map(DynamicFieldInfo::into_proto)
            .collect(),
        next_page_token,
    })
}

fn decode_page_token(page_token: &[u8]) -> Result<ObjectId> {
    Ok(ObjectId::new(page_token.try_into().unwrap()))
}

fn encode_page_token(page_token: ObjectId) -> Bytes {
    page_token.as_bytes().to_vec().into()
}

pub struct DynamicFieldInfo {
    pub parent: ObjectId,
    pub field_id: ObjectId,
    pub name_type: TypeTag,
    pub name_value: Vec<u8>,
    pub dynamic_object_id: Option<ObjectId>,
}

impl TryFrom<(DynamicFieldKey, DynamicFieldIndexInfo)> for DynamicFieldInfo {
    type Error = SdkTypeConversionError;

    fn try_from(value: (DynamicFieldKey, DynamicFieldIndexInfo)) -> Result<Self, Self::Error> {
        let DynamicFieldKey { parent, field_id } = value.0;
        let DynamicFieldIndexInfo {
            dynamic_field_type: _,
            name_type,
            name_value,
            dynamic_object_id,
        } = value.1;

        Self {
            parent: parent.into(),
            field_id: field_id.into(),
            name_type: type_tag_core_to_sdk(name_type)?,
            name_value,
            dynamic_object_id: dynamic_object_id.map(Into::into),
        }
        .pipe(Ok)
    }
}

impl DynamicFieldInfo {
    fn into_proto(self) -> DynamicField {
        DynamicField {
            parent: Some(self.parent.to_string()),
            field_id: Some(self.field_id.to_string()),
            name_type: Some(self.name_type.to_string()),
            name_value: Some(self.name_value.into()),
            dynamic_object_id: self.dynamic_object_id.map(|id| id.to_string()),
        }
    }
}
