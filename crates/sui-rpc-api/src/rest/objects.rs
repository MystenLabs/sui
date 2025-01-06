// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ApiEndpoint, RouteHandler};
use crate::types::{GetObjectOptions, ObjectResponse};
use crate::{reader::StateReader, rest::PageCursor, Result, RpcService, RpcServiceError};
use axum::extract::Query;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sui_sdk_types::{ObjectId, TypeTag, Version};
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

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_object)
    }
}

pub async fn get_object(
    Path(object_id): Path<ObjectId>,
    Query(options): Query<GetObjectOptions>,
    State(state): State<RpcService>,
) -> Result<Json<ObjectResponse>> {
    let object = state.get_object(object_id, None, options)?;

    Ok(Json(object))
}

pub struct GetObjectWithVersion;

impl ApiEndpoint<RpcService> for GetObjectWithVersion {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/objects/{object_id}/version/{version}"
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), get_object_with_version)
    }
}

pub async fn get_object_with_version(
    Path((object_id, version)): Path<(ObjectId, Version)>,
    Query(options): Query<GetObjectOptions>,
    State(state): State<RpcService>,
) -> Result<Json<ObjectResponse>> {
    let object = state.get_object(object_id, Some(version), options)?;

    Ok(Json(object))
}

pub struct ListDynamicFields;

impl ApiEndpoint<RpcService> for ListDynamicFields {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/objects/{object_id}/dynamic-fields"
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), list_dynamic_fields)
    }
}

async fn list_dynamic_fields(
    Path(parent): Path<ObjectId>,
    Query(parameters): Query<ListDynamicFieldsQueryParameters>,
    State(state): State<StateReader>,
) -> Result<(PageCursor<ObjectId>, Json<Vec<DynamicFieldInfo>>)> {
    let indexes = state
        .inner()
        .indexes()
        .ok_or_else(RpcServiceError::not_found)?;

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

    Ok((PageCursor(cursor), Json(dynamic_fields)))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
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

#[derive(Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq, Debug)]
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
