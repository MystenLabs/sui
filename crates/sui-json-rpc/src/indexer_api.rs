// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use jsonrpsee::core::error::SubscriptionClosed;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee::{RpcModule, SubscriptionSink};
use move_bytecode_utils::layout::TypeLayoutBuilder;
use move_core_types::account_address::AccountAddress;
use serde::Serialize;
use sui_json::SuiJsonValue;
use sui_types::error::SuiObjectResponseError;
use tracing::{debug, instrument, warn};

use move_core_types::ident_str;
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::{StructTag, TypeTag};
use mysten_metrics::spawn_monitored_task;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::{
    DynamicFieldPage, EventFilter, EventPage, ObjectsPage, Page, SuiMoveValue,
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery, SuiParsedMoveObject,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
    TransactionFilter,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::DynamicFieldName;
use sui_types::event::EventID;

use crate::api::{
    cap_page_limit, validate_limit, IndexerApiServer, JsonRpcMetrics, ReadApiServer,
    QUERY_MAX_RESULT_LIMIT,
};
use crate::error::{Error, SuiRpcInputError};
use crate::name_service::Domain;
use crate::with_tracing;
use crate::SuiRpcModule;

const NAME_SERVICE_VALUE: &str = "value";
const NAME_SERVICE_TARGET_ADDRESS: &str = "target_address";
const NAME_SERVICE_DOMAIN_MODULE: &IdentStr = ident_str!("domain");
const NAME_SERVICE_DOMAIN_STRUCT: &IdentStr = ident_str!("Domain");
const NAME_SERVICE_DEFAULT_PACKAGE_ADDRESS: &str =
    "0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0";
const NAME_SERVICE_DEFAULT_REGISTRY: &str =
    "0xe64cd9db9f829c6cc405d9790bd71567ae07259855f4fba6f02c84f52298c106";
const NAME_SERVICE_DEFAULT_REVERSE_REGISTRY: &str =
    "0x2fd099e17a292d2bc541df474f9fafa595653848cbabb2d7a4656ec786a1969f";

pub fn spawn_subscription<S, T>(mut sink: SubscriptionSink, rx: S)
where
    S: Stream<Item = T> + Unpin + Send + 'static,
    T: Serialize,
{
    spawn_monitored_task!(async move {
        match sink.pipe_from_stream(rx).await {
            SubscriptionClosed::Success => {
                debug!("Subscription completed.");
                sink.close(SubscriptionClosed::Success);
            }
            SubscriptionClosed::RemotePeerAborted => {
                debug!("Subscription aborted by remote peer.");
                sink.close(SubscriptionClosed::RemotePeerAborted);
            }
            SubscriptionClosed::Failed(err) => {
                debug!("Subscription failed: {err:?}");
                sink.close(err);
            }
        };
    });
}
pub struct IndexerApi<R> {
    state: Arc<AuthorityState>,
    read_api: R,
    ns_package_addr: Option<SuiAddress>,
    ns_registry_id: Option<ObjectID>,
    ns_reverse_registry_id: Option<ObjectID>,
    pub metrics: Arc<JsonRpcMetrics>,
}

impl<R: ReadApiServer> IndexerApi<R> {
    pub fn new(
        state: Arc<AuthorityState>,
        read_api: R,
        ns_package_addr: Option<SuiAddress>,
        ns_registry_id: Option<ObjectID>,
        ns_reverse_registry_id: Option<ObjectID>,
        metrics: Arc<JsonRpcMetrics>,
    ) -> Self {
        Self {
            state,
            read_api,
            ns_registry_id,
            ns_package_addr,
            ns_reverse_registry_id,
            metrics,
        }
    }
}

#[async_trait]
impl<R: ReadApiServer> IndexerApiServer for IndexerApi<R> {
    #[instrument(skip(self))]
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        with_tracing!(async move {
            let limit = validate_limit(limit, *QUERY_MAX_RESULT_LIMIT)?;
            self.metrics.get_owned_objects_limit.report(limit as u64);
            let SuiObjectResponseQuery { filter, options } = query.unwrap_or_default();
            let options = options.unwrap_or_default();
            let mut objects = self
                .state
                .get_owner_objects(address, cursor, limit + 1, filter)
                .map_err(Error::from)?;

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
                    self.read_api
                        .multi_get_objects(object_ids, Some(options))
                        .await?
                }
                false => objects
                    .into_iter()
                    .map(|o_info| SuiObjectResponse::try_from((o_info, options.clone())))
                    .collect::<Result<Vec<SuiObjectResponse>, _>>()?,
            };

            self.metrics
                .get_owned_objects_result_size
                .report(data.len() as u64);
            self.metrics
                .get_owned_objects_result_size_total
                .inc_by(data.len() as u64);
            Ok(Page {
                data,
                next_cursor,
                has_next_page,
            })
        })
    }

    #[instrument(skip(self))]
    async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage> {
        with_tracing!(async move {
            let limit = cap_page_limit(limit);
            self.metrics.query_tx_blocks_limit.report(limit as u64);
            let descending = descending_order.unwrap_or_default();
            let opts = query.options.unwrap_or_default();

            // Retrieve 1 extra item for next cursor
            let mut digests = self
                .state
                .get_transactions(query.filter, cursor, Some(limit + 1), descending)
                .map_err(Error::from)?;

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
                    .multi_get_transaction_blocks(digests, Some(opts))
                    .await?
            };

            self.metrics
                .query_tx_blocks_result_size
                .report(data.len() as u64);
            self.metrics
                .query_tx_blocks_result_size_total
                .inc_by(data.len() as u64);
            Ok(Page {
                data,
                next_cursor,
                has_next_page,
            })
        })
    }
    #[instrument(skip(self))]
    async fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        with_tracing!(async move {
            let descending = descending_order.unwrap_or_default();
            let limit = cap_page_limit(limit);
            self.metrics.query_events_limit.report(limit as u64);
            // Retrieve 1 extra item for next cursor
            let mut data = self
                .state
                .query_events(query, cursor.clone(), limit + 1, descending)
                .map_err(Error::from)?;
            let has_next_page = data.len() > limit;
            data.truncate(limit);
            let next_cursor = data.last().map_or(cursor, |e| Some(e.id.clone()));
            self.metrics
                .query_events_result_size
                .report(data.len() as u64);
            self.metrics
                .query_events_result_size_total
                .inc_by(data.len() as u64);
            Ok(EventPage {
                data,
                next_cursor,
                has_next_page,
            })
        })
    }

    #[instrument(skip(self))]
    fn subscribe_event(&self, sink: SubscriptionSink, filter: EventFilter) -> SubscriptionResult {
        spawn_subscription(
            sink,
            self.state.subscription_handler.subscribe_events(filter),
        );
        Ok(())
    }

    fn subscribe_transaction(
        &self,
        sink: SubscriptionSink,
        filter: TransactionFilter,
    ) -> SubscriptionResult {
        spawn_subscription(
            sink,
            self.state
                .subscription_handler
                .subscribe_transactions(filter),
        );
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        with_tracing!(async move {
            let limit = cap_page_limit(limit);
            self.metrics.get_dynamic_fields_limit.report(limit as u64);
            let mut data = self
                .state
                .get_dynamic_fields(parent_object_id, cursor, limit + 1)
                .map_err(Error::from)?;
            let has_next_page = data.len() > limit;
            data.truncate(limit);
            let next_cursor = data.last().cloned().map_or(cursor, |c| Some(c.0));
            self.metrics
                .get_dynamic_fields_result_size
                .report(data.len() as u64);
            self.metrics
                .get_dynamic_fields_result_size_total
                .inc_by(data.len() as u64);
            Ok(DynamicFieldPage {
                data: data.into_iter().map(|(_, w)| w).collect(),
                next_cursor,
                has_next_page,
            })
        })
    }

    #[instrument(skip(self))]
    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        with_tracing!(async move {
            let DynamicFieldName {
                type_: name_type,
                value,
            } = name.clone();
            let layout = TypeLayoutBuilder::build_with_types(&name_type, &self.state.database)?;
            let sui_json_value = SuiJsonValue::new(value)?;
            let name_bcs_value = sui_json_value.to_bcs_bytes(&layout)?;
            let id = self
                .state
                .get_dynamic_field_object_id(parent_object_id, name_type, &name_bcs_value)
                .map_err(Error::from)?;
            // TODO(chris): add options to `get_dynamic_field_object` API as well
            if let Some(id) = id {
                self.read_api
                    .get_object(id, Some(SuiObjectDataOptions::full_content()))
                    .await
                    .map_err(Error::from)
            } else {
                Ok(SuiObjectResponse::new_with_error(
                    SuiObjectResponseError::DynamicFieldNotFound { parent_object_id },
                ))
            }
        })
    }

    #[instrument(skip(self))]
    async fn resolve_name_service_address(&self, name: String) -> RpcResult<Option<SuiAddress>> {
        with_tracing!(async move {
            let pkg_addr = match self.ns_package_addr {
                Some(addr) => addr,
                None => SuiAddress::from_str(NAME_SERVICE_DEFAULT_PACKAGE_ADDRESS)?,
            };
            let registry_id = match self.ns_registry_id {
                Some(id) => id,
                None => ObjectID::from_str(NAME_SERVICE_DEFAULT_REGISTRY).map_err(|e| {
                    Error::UnexpectedError(format!(
                        "Parsing name service default registry ID failed with error: {:?}",
                        e
                    ))
                })?,
            };
            let package_addr = AccountAddress::new(pkg_addr.to_inner());
            let name_type_tag = TypeTag::Struct(Box::new(StructTag {
                address: package_addr,
                module: NAME_SERVICE_DOMAIN_MODULE.to_owned(),
                name: NAME_SERVICE_DOMAIN_STRUCT.to_owned(),
                type_params: vec![],
            }));
            let domain = Domain::from_str(&name).map_err(|e| {
                Error::UnexpectedError(format!(
                    "Failed to parse NameService Domain with error: {:?}",
                    e
                ))
            })?;
            let domain_bcs_value = bcs::to_bytes(&domain).map_err(|e| {
                Error::SuiRpcInputError(SuiRpcInputError::GenericInvalid(format!(
                    "Unable to serialize name: {:?} with error: {:?}",
                    domain, e
                )))
            })?;
            let record_object_id_option = self
                .state
                .get_dynamic_field_object_id(registry_id, name_type_tag, &domain_bcs_value)
                .map_err(|e| {
                    Error::SuiRpcInputError(SuiRpcInputError::GenericInvalid(format!(
                        "Unable to lookup name in name service registry with error: {:?}",
                        e
                    )))
                })?;
            if let Some(record_object_id) = record_object_id_option {
                let record_object_read =
                    self.state.get_object_read(&record_object_id).map_err(|e| {
                        Error::UnexpectedError(format!(
                            "Failed to get object read of name with error {:?}",
                            e
                        ))
                    })?;
                let record_parsed_move_object =
                    SuiParsedMoveObject::try_from_object_read(record_object_read)?;
                // NOTE: "value" is the field name to get the address info
                let address_info_move_value = record_parsed_move_object
                    .read_dynamic_field_value(NAME_SERVICE_VALUE)
                    .ok_or_else(|| {
                        Error::UnexpectedError(
                            "Cannot find value field in record Move struct".to_string(),
                        )
                    })?;
                let address_info_move_struct = match address_info_move_value {
                    SuiMoveValue::Struct(a) => Ok(a),
                    _ => Err(Error::UnexpectedError(
                        "value field is not found.".to_string(),
                    )),
                }?;
                // NOTE: "target_address" is the field name to get the address
                let address_str_move_value = address_info_move_struct
                    .read_dynamic_field_value(NAME_SERVICE_TARGET_ADDRESS)
                    .ok_or_else(|| {
                        Error::UnexpectedError(format!(
                            "Cannot find target_address field in address info Move struct: {:?}",
                            address_info_move_struct
                        ))
                    })?;
                let addr_opt = match &address_str_move_value {
                    SuiMoveValue::Option(boxed_addr) => match **boxed_addr {
                        Some(SuiMoveValue::Address(ref addr)) => Ok(Some(*addr)),
                        _ => Ok(None),
                    },
                    _ => Err(Error::UnexpectedError(format!(
                        "No SuiAddress found in: {:?}",
                        address_str_move_value
                    ))),
                }?;
                return Ok(addr_opt);
            }
            Ok(None)
        })
    }

    #[instrument(skip(self))]
    async fn resolve_name_service_names(
        &self,
        address: SuiAddress,
        _cursor: Option<ObjectID>,
        _limit: Option<usize>,
    ) -> RpcResult<Page<String, ObjectID>> {
        with_tracing!(async move {
            let reverse_registry_id = match self.ns_reverse_registry_id {
                Some(id) => id,
                None => ObjectID::from_str(NAME_SERVICE_DEFAULT_REVERSE_REGISTRY).map_err(|e| {
                    Error::UnexpectedError(format!(
                        "Parsing name service default reverse registry ID failed with error: {:?}",
                        e
                    ))
                })?,
            };

            let name_type_tag = TypeTag::Address;
            let addr_bcs_value = bcs::to_bytes(&address).map_err(|e| {
                Error::SuiRpcInputError(SuiRpcInputError::GenericInvalid(format!(
                    "Unable to serialize address: {:?} with error: {:?}",
                    address, e
                )))
            })?;

            let addr_object_id_opt = self
                .state
                .get_dynamic_field_object_id(reverse_registry_id, name_type_tag, &addr_bcs_value)
                .map_err(|e| {
                    Error::UnexpectedError(format!(
                        "Read name service reverse dynamic field table failed with error: {:?}",
                        e
                    ))
                })?;

            if let Some(addr_object_id) = addr_object_id_opt {
                let addr_object_read =
                    self.state.get_object_read(&addr_object_id).map_err(|e| {
                        warn!(
                            "Failed to get object read of address {:?} with error: {:?}",
                            addr_object_id, e
                        );
                        Error::UnexpectedError(format!(
                            "Failed to get object read of address with err: {:?}",
                            e
                        ))
                    })?;
                let addr_parsed_move_object =
                    SuiParsedMoveObject::try_from_object_read(addr_object_read)?;
                let address_info_move_value = addr_parsed_move_object
                    .read_dynamic_field_value(NAME_SERVICE_VALUE)
                    .ok_or_else(|| {
                        Error::UnexpectedError(
                            "Cannot find value field in record Move struct".to_string(),
                        )
                    })?;
                let domain_info_move_struct = match address_info_move_value {
                    SuiMoveValue::Struct(a) => Ok(a),
                    _ => Err(Error::UnexpectedError(
                        "value field is not found.".to_string(),
                    )),
                }?;
                let labels_move_value = domain_info_move_struct
                    .read_dynamic_field_value("labels")
                    .ok_or_else(|| {
                        Error::UnexpectedError(format!(
                            "Cannot find labels field in address info Move struct: {:?}",
                            domain_info_move_struct
                        ))
                    })?;
                let primary_domain_opt = match labels_move_value {
                    SuiMoveValue::Vector(labels) => {
                        let label_strs: Vec<String> = labels
                            .iter()
                            .rev()
                            .filter_map(|label| match label {
                                SuiMoveValue::String(label_str) => Some(label_str.clone()),
                                _ => None,
                            })
                            .collect();
                        Ok(if label_strs.is_empty() {
                            None
                        } else {
                            Some(label_strs.join("."))
                        })
                    }
                    _ => Err(Error::UnexpectedError(format!(
                        "No string field for primary name is found in {:?}",
                        labels_move_value
                    ))),
                }?;

                Ok(Page {
                    data: if let Some(primary_domain) = primary_domain_opt {
                        vec![primary_domain]
                    } else {
                        vec![]
                    },
                    next_cursor: Some(addr_object_id),
                    has_next_page: false,
                })
            } else {
                Ok(Page {
                    data: vec![],
                    next_cursor: None,
                    has_next_page: false,
                })
            }
        })
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
