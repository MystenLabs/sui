// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    accept::AcceptFormat,
    openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    reader::StateReader,
    response::ResponseContent,
    Page, RestError, RestService, Result,
};
use axum::extract::Query;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use sui_sdk2::types::{Object, ObjectId, TypeTag, Version};
use sui_types::storage::{DynamicFieldIndexInfo, DynamicFieldKey};
use sui_types::sui_sdk2_conversions::type_tag_core_to_sdk;
use tap::Pipe;

pub struct GetObject;

impl ApiEndpoint<RestService> for GetObject {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/objects/{object_id}"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Objects")
            .operation_id("GetObject")
            .path_parameter::<ObjectId>("object_id", generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Object>(generator)
                    .bcs_content()
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_object)
    }
}

pub async fn get_object(
    Path(object_id): Path<ObjectId>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<Object>> {
    let object = state
        .get_object(object_id)?
        .ok_or_else(|| ObjectNotFoundError::new(object_id))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
}

pub struct GetObjectWithVersion;

impl ApiEndpoint<RestService> for GetObjectWithVersion {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/objects/{object_id}/version/{version}"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Objects")
            .operation_id("GetObjectWithVersion")
            .path_parameter::<ObjectId>("object_id", generator)
            .path_parameter::<Version>("version", generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Object>(generator)
                    .bcs_content()
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), get_object_with_version)
    }
}

pub async fn get_object_with_version(
    Path((object_id, version)): Path<(ObjectId, Version)>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<ResponseContent<Object>> {
    let object = state
        .get_object_with_version(object_id, version)?
        .ok_or_else(|| ObjectNotFoundError::new_with_version(object_id, version))?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object),
    }
    .pipe(Ok)
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

impl From<ObjectNotFoundError> for crate::RestError {
    fn from(value: ObjectNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}

pub struct ListDynamicFields;

impl ApiEndpoint<RestService> for ListDynamicFields {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/objects/{object_id}/dynamic-fields"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Objects")
            .operation_id("ListDynamicFields")
            .path_parameter::<ObjectId>("object_id", generator)
            .query_parameters::<ListDynamicFieldsQueryParameters>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Vec<DynamicFieldInfo>>(generator)
                    .header::<String>(crate::types::X_SUI_CURSOR, generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), list_dynamic_fields)
    }
}

async fn list_dynamic_fields(
    Path(parent): Path<ObjectId>,
    Query(parameters): Query<ListDynamicFieldsQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Page<DynamicFieldInfo, ObjectId>> {
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RestError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let limit = parameters.limit();
    let start = parameters.start();

    let mut dynamic_fields = state
        .inner()
        .dynamic_field_iter(parent.into(), start)?
        .take(limit + 1)
        .map(DynamicFieldInfo::from)
        .collect::<Vec<_>>();

    let cursor = if dynamic_fields.len() > limit {
        // SAFETY: We've already verified that object_keys is greater than limit, which is
        // gaurenteed to be >= 1.
        dynamic_fields
            .pop()
            .unwrap()
            .field_id
            .pipe(ObjectId::from)
            .pipe(Some)
    } else {
        None
    };

    ResponseContent::Json(dynamic_fields)
        .pipe(|entries| Page { entries, cursor })
        .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ListDynamicFieldsQueryParameters {
    pub limit: Option<u32>,
    pub start: Option<ObjectId>,
}

impl ListDynamicFieldsQueryParameters {
    pub fn limit(&self) -> usize {
        self.limit
            .map(|l| (l as usize).clamp(1, crate::MAX_PAGE_SIZE))
            .unwrap_or(crate::DEFAULT_PAGE_SIZE)
    }

    pub fn start(&self) -> Option<sui_types::base_types::ObjectID> {
        self.start.map(Into::into)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug, schemars::JsonSchema)]
/// DynamicFieldInfo
pub struct DynamicFieldInfo {
    pub parent: ObjectId,
    pub field_id: ObjectId,
    pub dynamic_field_type: DynamicFieldType,
    pub name_type: TypeTag,
    //TODO fix the json format of this type to be base64 encoded
    pub name_value: Vec<u8>,
    /// ObjectId of the child object when `dynamic_field_type == DynamicFieldType::Object`
    pub dynamic_object_id: Option<ObjectId>,
}

impl From<(DynamicFieldKey, DynamicFieldIndexInfo)> for DynamicFieldInfo {
    fn from(value: (DynamicFieldKey, DynamicFieldIndexInfo)) -> Self {
        let DynamicFieldKey { parent, field_id } = value.0;
        let DynamicFieldIndexInfo {
            dynamic_field_type,
            name_type,
            name_value,
            dynamic_object_id,
        } = value.1;

        Self {
            parent: parent.into(),
            field_id: field_id.into(),
            dynamic_field_type: dynamic_field_type.into(),
            name_type: type_tag_core_to_sdk(name_type),
            name_value,
            dynamic_object_id: dynamic_object_id.map(Into::into),
        }
    }
}

#[derive(
    Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum DynamicFieldType {
    Field,
    Object,
}

impl From<sui_types::dynamic_field::DynamicFieldType> for DynamicFieldType {
    fn from(value: sui_types::dynamic_field::DynamicFieldType) -> Self {
        match value {
            sui_types::dynamic_field::DynamicFieldType::DynamicField => Self::Field,
            sui_types::dynamic_field::DynamicFieldType::DynamicObject => Self::Object,
        }
    }
}
