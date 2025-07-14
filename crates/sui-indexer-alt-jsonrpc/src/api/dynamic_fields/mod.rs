// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{DynamicFieldInfo as DynamicFieldInfoResponse, Page, SuiObjectResponse};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{base_types::ObjectID, dynamic_field::DynamicFieldName};

use crate::{api::objects, context::Context, error::InternalContext};

use super::rpc_module::RpcModule;

mod error;
mod response;

#[open_rpc(namespace = "suix", tag = "Dynamic Fields API")]
#[rpc(server, namespace = "suix")]
trait DynamicFieldsApi {
    /// Return the information from a dynamic field based on its parent ID and name.
    #[method(name = "getDynamicFieldObject")]
    async fn get_dynamic_field_object(
        &self,
        /// The ID of the parent object
        parent_object_id: ObjectID,
        /// The Name of the dynamic field
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse>;

    /// Query dynamic fields by their parent object. Returns a paginated list of object info.
    ///
    /// If a cursor is provided, the query will start from the dynamic field after the one pointed
    /// to by this cursor, otherwise pagination starts from the first page of dynamic fields
    /// owned by the object.
    ///
    /// The size of each page is controlled by the `limit` parameter.
    #[method(name = "getDynamicFields")]
    async fn get_dynamic_fields(
        &self,
        /// The ID of the parent object
        parent_object_id: ObjectID,
        /// Cursor to start paginating from.
        cursor: Option<String>,
        /// Maximum number of objects to return per page.
        limit: Option<usize>,
    ) -> RpcResult<Page<DynamicFieldInfoResponse, String>>;
}

pub struct DynamicFields(pub Context);

#[async_trait::async_trait]
impl DynamicFieldsApiServer for DynamicFields {
    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        let Self(ctx) = self;
        Ok(response::dynamic_field_object(ctx, parent_object_id, name).await?)
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> RpcResult<Page<DynamicFieldInfoResponse, String>> {
        let Self(ctx) = self;

        let Page {
            data: object_ids,
            next_cursor,
            has_next_page,
        } = objects::filter::dynamic_fields(ctx, parent_object_id, cursor, limit).await?;

        let df_futures = object_ids
            .iter()
            .map(|id| response::dynamic_field_info(ctx, *id));

        let data = future::join_all(df_futures)
            .await
            .into_iter()
            .zip(object_ids)
            .map(|(r, id)| {
                r.with_internal_context(|| format!("Failed to get object {id} at latest version"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            data,
            next_cursor: next_cursor.map(|id| id.to_string()),
            has_next_page,
        })
    }
}

impl RpcModule for DynamicFields {
    fn schema(&self) -> Module {
        DynamicFieldsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
