// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::node::v2alpha::DynamicField;
use crate::proto::node::v2alpha::ListDynamicFieldsRequest;
use crate::proto::node::v2alpha::ListDynamicFieldsResponse;
use crate::types::GetObjectOptions;
use crate::types::ObjectResponse;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_sdk_types::ObjectId;
use sui_sdk_types::TypeTag;
use sui_sdk_types::Version;
use sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk;
use sui_types::{
    storage::{DynamicFieldIndexInfo, DynamicFieldKey},
    sui_sdk_types_conversions::SdkTypeConversionError,
};
use tap::Pipe;

impl RpcService {
    pub fn get_object(
        &self,
        object_id: ObjectId,
        version: Option<Version>,
        options: GetObjectOptions,
    ) -> Result<ObjectResponse> {
        let object = if let Some(version) = version {
            self.reader
                .get_object_with_version(object_id, version)?
                .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?
        } else {
            self.reader
                .get_object(object_id)?
                .ok_or_else(|| ObjectNotFoundError::new(object_id))?
        };

        let object_bcs = options
            .include_object_bcs()
            .then(|| bcs::to_bytes(&object))
            .transpose()?;

        ObjectResponse {
            object_id: object.object_id(),
            version: object.version(),
            digest: object.digest(),
            object: options.include_object().then_some(object),
            object_bcs,
        }
        .pipe(Ok)
    }
}

#[derive(Debug)]
pub struct ObjectNotFoundError {
    object_id: ObjectId,
    version: Option<Version>,
}

impl ObjectNotFoundError {
    pub fn new(object_id: ObjectId) -> Self {
        Self {
            object_id,
            version: None,
        }
    }

    pub fn new_with_version(object_id: ObjectId, version: Version) -> Self {
        Self {
            object_id,
            version: Some(version),
        }
    }
}

impl std::fmt::Display for ObjectNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object {}", self.object_id)?;

        if let Some(version) = self.version {
            write!(f, " with version {version}")?;
        }

        write!(f, " not found")
    }
}

impl std::error::Error for ObjectNotFoundError {}

impl From<ObjectNotFoundError> for crate::RpcError {
    fn from(value: ObjectNotFoundError) -> Self {
        Self::new(tonic::Code::NotFound, value.to_string())
    }
}

impl RpcService {
    pub fn list_dynamic_fields(
        &self,
        request: ListDynamicFieldsRequest,
    ) -> Result<ListDynamicFieldsResponse> {
        let indexes = self
            .reader
            .inner()
            .indexes()
            .ok_or_else(RpcError::not_found)?;

        let parent: ObjectId = request
            .parent
            .as_ref()
            .ok_or_else(|| RpcError::new(tonic::Code::InvalidArgument, "missing parent"))?
            .try_into()
            .map_err(|e| {
                RpcError::new(tonic::Code::InvalidArgument, format!("invalid parent: {e}"))
            })?;

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
            .map(DynamicFieldInfo::try_from)
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
}

fn decode_page_token(page_token: &str) -> Result<ObjectId> {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    let bytes = BASE64_STANDARD.decode(page_token).unwrap();
    Ok(ObjectId::new(bytes.try_into().unwrap()))
}

fn encode_page_token(page_token: ObjectId) -> String {
    use base64::prelude::BASE64_STANDARD;
    use base64::Engine;

    BASE64_STANDARD.encode(page_token.as_bytes())
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
            parent: Some(self.parent.into()),
            field_id: Some(self.field_id.into()),
            name_type: Some(self.name_type.into()),
            name_value: Some(self.name_value.into()),
            dynamic_object_id: self.dynamic_object_id.map(Into::into),
        }
    }
}
