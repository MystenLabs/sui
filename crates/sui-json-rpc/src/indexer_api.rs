// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use futures::Stream;
use jsonrpsee::core::error::SubscriptionClosed;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};
use serde::Serialize;
use tracing::{debug, warn};

use move_core_types::language_storage::TypeTag;
use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    DynamicFieldPage, EventFilter, EventPage, ObjectsPage, Page, SuiMoveValue,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiParsedMoveObject,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::event::EventID;

use crate::api::{
    cap_page_limit, validate_limit, IndexerApiServer, ReadApiServer, QUERY_MAX_RESULT_LIMIT_OBJECTS,
};
use crate::SuiRpcModule;

pub fn spawn_subscription<S, T>(mut sink: SubscriptionSink, rx: S)
where
    S: Stream<Item = T> + Unpin + Send + 'static,
    T: Serialize,
{
    spawn_monitored_task!(async move {
        match sink.pipe_from_stream(rx).await {
            SubscriptionClosed::Success => {
                sink.close(SubscriptionClosed::Success);
            }
            SubscriptionClosed::RemotePeerAborted => (),
            SubscriptionClosed::Failed(err) => {
                warn!(error = ?err, "Event subscription closed.");
                sink.close(err);
            }
        };
    });
}

pub struct IndexerApi<R> {
    state: Arc<AuthorityState>,
    read_api: R,
}

impl<R: ReadApiServer> IndexerApi<R> {
    pub fn new(state: Arc<AuthorityState>, read_api: R) -> Self {
        Self { state, read_api }
    }
}

#[async_trait]
impl<R: ReadApiServer> IndexerApiServer for IndexerApi<R> {
    fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_OBJECTS)?;
        let SuiObjectResponseQuery { filter, options } = query.unwrap_or_default();
        let options = options.unwrap_or_default();
        let mut objects = self
            .state
            .get_owner_objects(address, cursor, limit + 1, filter)
            .map_err(|e| anyhow!("{e}"))?;

        // objects here are of size (limit + 1), where the last one is the cursor for the next page
        let has_next_page = objects.len() > limit;
        objects.truncate(limit);
        let next_cursor = objects
            .last()
            .cloned()
            .map_or(cursor, |o_info| Some(o_info.object_id));

        let data = match options.is_not_in_object_info() {
            true => {
                let object_ids = objects.iter().map(|obj| obj.object_id).collect();
                self.read_api.multi_get_objects(object_ids, Some(options))?
            }
            false => objects
                .into_iter()
                .map(|o_info| SuiObjectResponse::try_from((o_info, options.clone())))
                .collect::<Result<Vec<SuiObjectResponse>, _>>()?,
        };

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }

    fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage> {
        let limit = cap_page_limit(limit);
        let descending = descending_order.unwrap_or_default();
        let opts = query.options.unwrap_or_default();

        // Retrieve 1 extra item for next cursor
        let mut digests =
            self.state
                .get_transactions(query.filter, cursor, Some(limit + 1), descending)?;

        // extract next cursor
        let has_next_page = digests.len() > limit;
        digests.truncate(limit);
        let next_cursor = digests.last().cloned().map_or(cursor, Some);

        let data: Vec<SuiTransactionBlockResponse> = if opts.only_digest() {
            digests
                .into_iter()
                .map(SuiTransactionBlockResponse::new)
                .collect()
        } else {
            self.read_api
                .multi_get_transaction_blocks(digests, Some(opts))?
        };

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }
    fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        debug!(
            ?query,
            ?cursor,
            ?limit,
            ?descending_order,
            "get_events query"
        );
        let descending = descending_order.unwrap_or_default();
        let limit = cap_page_limit(limit);
        // Retrieve 1 extra item for next cursor
        let mut data = self
            .state
            .query_events(query, cursor.clone(), limit + 1, descending)?;
        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor = data.last().map_or(cursor, |e| Some(e.id.clone()));
        Ok(EventPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    fn subscribe_event(&self, sink: SubscriptionSink, filter: EventFilter) -> SubscriptionResult {
        spawn_subscription(sink, self.state.event_handler.subscribe(filter));
        Ok(())
    }

    fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        let limit = cap_page_limit(limit);
        let mut data = self
            .state
            .get_dynamic_fields(parent_object_id, cursor, limit + 1)
            .map_err(|e| anyhow!("{e}"))?;
        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor = data.last().cloned().map_or(cursor, |c| Some(c.object_id));
        Ok(DynamicFieldPage {
            data,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        let id = self
            .state
            .get_dynamic_field_object_id(parent_object_id, &name)
            .map_err(|e| anyhow!("{e}"))?
            .ok_or_else(|| {
                anyhow!("Cannot find dynamic field [{name:?}] for object [{parent_object_id}].")
            })?;
        // TODO(chris): add options to `get_dynamic_field_object` API as well
        self.read_api
            .get_object(id, Some(SuiObjectDataOptions::full_content()))
    }

    async fn resolve_name_service_address(
        &self,
        resolver_id: ObjectID,
        name: String,
    ) -> RpcResult<SuiAddress> {
        let resolver_object_read = self.state.get_object_read(&resolver_id).map_err(|e| {
            warn!(
                ?resolver_id,
                "Failed to get object read of resolver {:?} with error: {:?}", resolver_id, e
            );
            anyhow!("{e}")
        })?;
        let resolved_parsed_move_object =
            SuiParsedMoveObject::try_from_object_read(resolver_object_read)?;
        // NOTE: "records" is the field name to get "records" Move struct
        let records_value = resolved_parsed_move_object
            .read_dynamic_field_value("records")
            .ok_or_else(|| anyhow!("Cannot find records field in resolved object"))?;
        let records_move_struct = match records_value {
            SuiMoveValue::Struct(s) => Ok(s),
            _ => Err(anyhow!("records field is not a Move struct")),
        }?;
        // NOTE: "id" is the field name to get dynamic field table object ID
        let dynmaic_field_table_object_id_struct = records_move_struct
            .read_dynamic_field_value("id")
            .ok_or_else(|| anyhow!("Cannot find id field in records Move struct"))?;
        let dynmaic_field_table_object_id = match dynmaic_field_table_object_id_struct {
            SuiMoveValue::UID { id } => Ok(id),
            _ => Err(anyhow!("id field is not a UID")),
        }?;

        // NOTE: 0x1::string::String is the type tag of fields in dynmaic_field_table
        let ns_type_tag = TypeTag::from_str("0x1::string::String")?;
        let ns_dynamic_field_name = DynamicFieldName {
            type_: ns_type_tag,
            value: name.clone().into(),
        };

        // record of the input `name`
        let record_object_id = self
            .state
            .get_dynamic_field_object_id(dynmaic_field_table_object_id, &ns_dynamic_field_name)
            .map_err(|e| {
                anyhow!(
                    "Read name service dynamic field table failed with error: {:?}",
                    e
                )
            })?
            .ok_or_else(|| {
                anyhow!(
                    "Record not found for name: {:?} in name service: {:?}",
                    name,
                    resolver_id
                )
            })?;
        let record_object_read = self.state.get_object_read(&record_object_id).map_err(|e| {
            warn!(
                ?record_object_id,
                "Failed to get object read of record {:?} with error: {:?}", record_object_id, e
            );
            anyhow!("{e}")
        })?;
        let record_parsed_move_object =
            SuiParsedMoveObject::try_from_object_read(record_object_read)?;
        // NOTE: "value" is the field name to get the address info
        let address_info_move_value = record_parsed_move_object
            .read_dynamic_field_value("value")
            .ok_or_else(|| anyhow!("Cannot find value field in record Move struct"))?;
        let address_info_move_struct = match address_info_move_value {
            SuiMoveValue::Struct(a) => Ok(a),
            _ => Err(anyhow!("value field is not found.")),
        }?;
        // NOTE: "by" is the field name to get the address
        let address_str_move_value = address_info_move_struct
            .read_dynamic_field_value("by")
            .ok_or_else(|| anyhow!("Cannot find by field in address info Move struct"))?;
        let address_str = match address_str_move_value {
            SuiMoveValue::String(s) => Ok(s),
            _ => Err(anyhow!("by field is not found.")),
        }?;
        let sui_addr = SuiAddress::from_str(&address_str)?;
        Ok(sui_addr)
    }
}

impl<R: ReadApiServer> SuiRpcModule for IndexerApi<R> {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::IndexerApiOpenRpc::module_doc()
    }
}
