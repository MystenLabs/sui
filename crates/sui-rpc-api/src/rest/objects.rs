// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::{GetObjectOptions, ObjectResponse};
use crate::{
    reader::StateReader,
    response::ResponseContent,
    rest::accept::AcceptFormat,
    rest::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler},
    rest::Page,
    Result, RpcService, RpcServiceError,
};
use axum::extract::Query;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use sui_sdk_types::types::{Object, ObjectId, TypeTag, Version};
use sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk;
use sui_types::{
    storage::{DynamicFieldIndexInfo, DynamicFieldKey},
    sui_sdk_types_conversions::SdkTypeConversionError,
};
use tap::Pipe;

pub struct GetObject;

impl ApiEndpoint<RpcService> for GetObject {
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
            .query_parameters::<GetObjectOptions>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<ObjectResponse>(generator)
                    .bcs_content()
                    .build(),
            )
            .response(404, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> crate::rest::openapi::RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_object)
    }
}

pub async fn get_object(
    Path(object_id): Path<ObjectId>,
    Query(options): Query<GetObjectOptions>,
    accept: AcceptFormat,
    State(state): State<RpcService>,
) -> Result<ResponseContent<Object, ObjectResponse>> {
    let object = state.get_object(object_id, None, options)?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object.object.unwrap()),
    }
    .pipe(Ok)
}

pub struct GetObjectWithVersion;

impl ApiEndpoint<RpcService> for GetObjectWithVersion {
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
            .query_parameters::<GetObjectOptions>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<ObjectResponse>(generator)
                    .bcs_content()
                    .build(),
            )
            .response(404, ResponseBuilder::new().build())
            .build()
    }

    fn handler(&self) -> crate::rest::openapi::RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_object_with_version)
    }
}

pub async fn get_object_with_version(
    Path((object_id, version)): Path<(ObjectId, Version)>,
    Query(options): Query<GetObjectOptions>,
    accept: AcceptFormat,
    State(state): State<RpcService>,
) -> Result<ResponseContent<Object, ObjectResponse>> {
    let object = state.get_object(object_id, Some(version), options)?;

    match accept {
        AcceptFormat::Json => ResponseContent::Json(object),
        AcceptFormat::Bcs => ResponseContent::Bcs(object.object.unwrap()),
    }
    .pipe(Ok)
}

pub struct ListDynamicFields;

impl ApiEndpoint<RpcService> for ListDynamicFields {
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

    fn handler(&self) -> crate::rest::openapi::RouteHandler<RpcService> {
        RouteHandler::new(self.method(), list_dynamic_fields)
    }
}

async fn list_dynamic_fields(
    Path(parent): Path<ObjectId>,
    Query(parameters): Query<ListDynamicFieldsQueryParameters>,
    accept: AcceptFormat,
    State(state): State<StateReader>,
) -> Result<Page<DynamicFieldInfo, ObjectId>> {
    let indexes = state
        .inner()
        .indexes()
        .ok_or_else(RpcServiceError::not_found)?;
    match accept {
        AcceptFormat::Json => {}
        _ => {
            return Err(RpcServiceError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid accept type",
            ))
        }
    }

    let limit = parameters.limit();
    let start = parameters.start();

    let mut dynamic_fields = indexes
        .dynamic_field_iter(parent.into(), start)?
        .take(limit + 1)
        .map(DynamicFieldInfo::try_from)
        .collect::<Result<Vec<_>, _>>()?;

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
            .map(|l| (l as usize).clamp(1, crate::rest::MAX_PAGE_SIZE))
            .unwrap_or(crate::rest::DEFAULT_PAGE_SIZE)
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

impl TryFrom<(DynamicFieldKey, DynamicFieldIndexInfo)> for DynamicFieldInfo {
    type Error = SdkTypeConversionError;

    fn try_from(value: (DynamicFieldKey, DynamicFieldIndexInfo)) -> Result<Self, Self::Error> {
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
            name_type: type_tag_core_to_sdk(name_type)?,
            name_value,
            dynamic_object_id: dynamic_object_id.map(Into::into),
        }
        .pipe(Ok)
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
