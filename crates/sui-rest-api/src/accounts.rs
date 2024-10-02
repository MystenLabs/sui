// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::openapi::{ApiEndpoint, OperationBuilder, ResponseBuilder, RouteHandler};
use crate::reader::StateReader;
use crate::{response::ResponseContent, Result};
use crate::{Page, RestService};
use axum::extract::Query;
use axum::extract::{Path, State};
use openapiv3::v3_1::Operation;
use sui_sdk_types::types::{Address, ObjectId, StructTag, Version};
use sui_types::sui_sdk_types_conversions::struct_tag_core_to_sdk;
use tap::Pipe;

pub struct ListAccountObjects;

impl ApiEndpoint<RestService> for ListAccountObjects {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/accounts/{account}/objects"
    }

    fn operation(&self, generator: &mut schemars::gen::SchemaGenerator) -> Operation {
        OperationBuilder::new()
            .tag("Account")
            .operation_id("ListAccountObjects")
            .path_parameter::<Address>("account", generator)
            .query_parameters::<ListAccountOwnedObjectsQueryParameters>(generator)
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<Vec<AccountOwnedObjectInfo>>(generator)
                    .header::<String>(crate::types::X_SUI_CURSOR, generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> crate::openapi::RouteHandler<RestService> {
        RouteHandler::new(self.method(), list_account_objects)
    }
}

async fn list_account_objects(
    Path(address): Path<Address>,
    Query(parameters): Query<ListAccountOwnedObjectsQueryParameters>,
    State(state): State<StateReader>,
) -> Result<Page<AccountOwnedObjectInfo, ObjectId>> {
    let limit = parameters.limit();
    let start = parameters.start();

    let mut object_info = state
        .inner()
        .account_owned_objects_info_iter(address.into(), start)?
        .take(limit + 1)
        .map(|info| {
            AccountOwnedObjectInfo {
                owner: info.owner.into(),
                object_id: info.object_id.into(),
                version: info.version.into(),
                type_: struct_tag_core_to_sdk(info.type_.into())?,
            }
            .pipe(Ok)
        })
        .collect::<Result<Vec<_>>>()?;

    let cursor = if object_info.len() > limit {
        // SAFETY: We've already verified that object_info is greater than limit, which is
        // gaurenteed to be >= 1.
        object_info.pop().unwrap().object_id.pipe(Some)
    } else {
        None
    };

    object_info
        .pipe(ResponseContent::Json)
        .pipe(|entries| Page { entries, cursor })
        .pipe(Ok)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ListAccountOwnedObjectsQueryParameters {
    pub limit: Option<u32>,
    pub start: Option<ObjectId>,
}

impl ListAccountOwnedObjectsQueryParameters {
    pub fn limit(&self) -> usize {
        self.limit
            .map(|l| (l as usize).clamp(1, crate::MAX_PAGE_SIZE))
            .unwrap_or(crate::DEFAULT_PAGE_SIZE)
    }

    pub fn start(&self) -> Option<sui_types::base_types::ObjectID> {
        self.start.map(Into::into)
    }
}

#[serde_with::serde_as]
#[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct AccountOwnedObjectInfo {
    pub owner: Address,
    pub object_id: ObjectId,
    #[serde_as(as = "sui_types::sui_serde::BigInt<u64>")]
    #[schemars(with = "crate::_schemars::U64")]
    pub version: Version,
    #[serde(rename = "type")]
    pub type_: StructTag,
}
