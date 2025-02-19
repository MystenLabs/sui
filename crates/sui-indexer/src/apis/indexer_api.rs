// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::core::SubscriptionResult;
use jsonrpsee::{PendingSubscriptionSink, RpcModule};
use tap::TapFallible;

use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{cap_page_limit, IndexerApiServer};
use sui_json_rpc_types::{
    DynamicFieldPage, EventFilter, EventPage, ObjectsPage, Page, SuiObjectResponse,
    SuiObjectResponseQuery, SuiTransactionBlockResponseQuery, TransactionBlocksPage,
    TransactionFilter,
};
use sui_name_service::{Domain, NameRecord, NameServiceConfig, NameServiceError};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::dynamic_field::{DynamicFieldName, Field};
use sui_types::error::SuiObjectResponseError;
use sui_types::event::EventID;
use sui_types::object::ObjectRead;
use sui_types::TypeTag;

use crate::indexer_reader::IndexerReader;
use crate::IndexerError;

pub(crate) struct IndexerApi {
    inner: IndexerReader,
    name_service_config: NameServiceConfig,
}

impl IndexerApi {
    pub fn new(inner: IndexerReader, name_service_config: NameServiceConfig) -> Self {
        Self {
            inner,
            name_service_config,
        }
    }

    async fn get_owned_objects_internal(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> RpcResult<ObjectsPage> {
        let SuiObjectResponseQuery { filter, options } = query.unwrap_or_default();
        let options = options.unwrap_or_default();
        let objects = self
            .inner
            .get_owned_objects(address, filter, cursor, limit + 1)
            .await?;

        let mut object_futures = vec![];
        for object in objects {
            object_futures.push(tokio::task::spawn(
                object.try_into_object_read(self.inner.package_resolver()),
            ));
        }
        let mut objects = futures::future::join_all(object_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                tracing::error!("Error joining object read futures.");
                crate::errors::IndexerError::from(e)
            })?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .tap_err(|e| tracing::error!("Error converting object to object read: {}", e))?;
        let has_next_page = objects.len() > limit;
        objects.truncate(limit);

        let next_cursor = objects.last().map(|o_read| o_read.object_id());
        let mut parallel_tasks = vec![];
        for o in objects {
            let inner_clone = self.inner.clone();
            let options = options.clone();
            parallel_tasks.push(tokio::task::spawn(async move {
                match o {
                    ObjectRead::NotExists(id) => Ok(SuiObjectResponse::new_with_error(
                        SuiObjectResponseError::NotExists { object_id: id },
                    )),
                    ObjectRead::Exists(object_ref, o, layout) => {
                        if options.show_display {
                            match inner_clone.get_display_fields(&o, &layout).await {
                                Ok(rendered_fields) => Ok(SuiObjectResponse::new_with_data(
                                    (object_ref, o, layout, options, Some(rendered_fields))
                                        .try_into()?,
                                )),
                                Err(e) => Ok(SuiObjectResponse::new(
                                    Some((object_ref, o, layout, options, None).try_into()?),
                                    Some(SuiObjectResponseError::DisplayError {
                                        error: e.to_string(),
                                    }),
                                )),
                            }
                        } else {
                            Ok(SuiObjectResponse::new_with_data(
                                (object_ref, o, layout, options, None).try_into()?,
                            ))
                        }
                    }
                    ObjectRead::Deleted((object_id, version, digest)) => Ok(
                        SuiObjectResponse::new_with_error(SuiObjectResponseError::Deleted {
                            object_id,
                            version,
                            digest,
                        }),
                    ),
                }
            }));
        }
        let data = futures::future::join_all(parallel_tasks)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: tokio::task::JoinError| anyhow::anyhow!(e))
            .map_err(IndexerError::from)?
            .into_iter()
            .collect::<Result<Vec<_>, anyhow::Error>>()
            .map_err(IndexerError::from)?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }
}

#[async_trait]
impl IndexerApiServer for IndexerApi {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<ObjectsPage> {
        let limit = cap_page_limit(limit);
        if limit == 0 {
            return Ok(ObjectsPage::empty());
        }
        self.get_owned_objects_internal(address, query, cursor, limit)
            .await
    }

    async fn query_transaction_blocks(
        &self,
        query: SuiTransactionBlockResponseQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<TransactionBlocksPage> {
        let limit = cap_page_limit(limit);
        if limit == 0 {
            return Ok(TransactionBlocksPage::empty());
        }
        let mut results = self
            .inner
            .query_transaction_blocks(
                query.filter,
                query.options.unwrap_or_default(),
                cursor,
                limit + 1,
                descending_order.unwrap_or(false),
            )
            .await?;

        let has_next_page = results.len() > limit;
        results.truncate(limit);
        let next_cursor = results.last().map(|o| o.digest);
        Ok(Page {
            data: results,
            next_cursor,
            has_next_page,
        })
    }

    async fn query_events(
        &self,
        query: EventFilter,
        // exclusive cursor if `Some`, otherwise start from the beginning
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EventPage> {
        let limit = cap_page_limit(limit);
        if limit == 0 {
            return Ok(EventPage::empty());
        }
        let descending_order = descending_order.unwrap_or(false);
        let mut results = self
            .inner
            .query_events(query, cursor, limit + 1, descending_order)
            .await?;

        let has_next_page = results.len() > limit;
        results.truncate(limit);
        let next_cursor = results.last().map(|o| o.id);
        Ok(Page {
            data: results,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_dynamic_fields(
        &self,
        parent_object_id: ObjectID,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<DynamicFieldPage> {
        let limit = cap_page_limit(limit);
        if limit == 0 {
            return Ok(DynamicFieldPage::empty());
        }
        let mut results = self
            .inner
            .get_dynamic_fields(parent_object_id, cursor, limit + 1)
            .await?;

        let has_next_page = results.len() > limit;
        results.truncate(limit);
        let next_cursor = results.last().map(|o| o.object_id);
        Ok(Page {
            data: results.into_iter().map(Into::into).collect(),
            next_cursor,
            has_next_page,
        })
    }

    async fn get_dynamic_field_object(
        &self,
        parent_object_id: ObjectID,
        name: DynamicFieldName,
    ) -> RpcResult<SuiObjectResponse> {
        let name_bcs_value = self.inner.bcs_name_from_dynamic_field_name(&name).await?;
        // Try as Dynamic Field
        let id = sui_types::dynamic_field::derive_dynamic_field_id(
            parent_object_id,
            &name.type_,
            &name_bcs_value,
        )
        .expect("deriving dynamic field id can't fail");

        let options = sui_json_rpc_types::SuiObjectDataOptions::full_content();
        match self.inner.get_object_read(id).await? {
            sui_types::object::ObjectRead::NotExists(_)
            | sui_types::object::ObjectRead::Deleted(_) => {}
            sui_types::object::ObjectRead::Exists(object_ref, o, layout) => {
                return Ok(SuiObjectResponse::new_with_data(
                    (object_ref, o, layout, options, None)
                        .try_into()
                        .map_err(IndexerError::from)?,
                ));
            }
        }

        // Try as Dynamic Field Object
        let dynamic_object_field_struct =
            sui_types::dynamic_field::DynamicFieldInfo::dynamic_object_field_wrapper(name.type_);
        let dynamic_object_field_type = TypeTag::Struct(Box::new(dynamic_object_field_struct));
        let dynamic_object_field_id = sui_types::dynamic_field::derive_dynamic_field_id(
            parent_object_id,
            &dynamic_object_field_type,
            &name_bcs_value,
        )
        .expect("deriving dynamic field id can't fail");
        match self.inner.get_object_read(dynamic_object_field_id).await? {
            sui_types::object::ObjectRead::NotExists(_)
            | sui_types::object::ObjectRead::Deleted(_) => {}
            sui_types::object::ObjectRead::Exists(object_ref, o, layout) => {
                return Ok(SuiObjectResponse::new_with_data(
                    (object_ref, o, layout, options, None)
                        .try_into()
                        .map_err(IndexerError::from)?,
                ));
            }
        }

        Ok(SuiObjectResponse::new_with_error(
            sui_types::error::SuiObjectResponseError::DynamicFieldNotFound { parent_object_id },
        ))
    }

    fn subscribe_event(
        &self,
        _sink: PendingSubscriptionSink,
        _filter: EventFilter,
    ) -> SubscriptionResult {
        Err("disabled".into())
    }

    fn subscribe_transaction(
        &self,
        _sink: PendingSubscriptionSink,
        _filter: TransactionFilter,
    ) -> SubscriptionResult {
        Err("disabled".into())
    }

    async fn resolve_name_service_address(&self, name: String) -> RpcResult<Option<SuiAddress>> {
        let domain: Domain = name.parse().map_err(IndexerError::NameServiceError)?;
        let parent_domain = domain.parent();

        // construct the record ids to lookup.
        let record_id = self.name_service_config.record_field_id(&domain);
        let parent_record_id = self.name_service_config.record_field_id(&parent_domain);

        // get latest timestamp to check expiration.
        let current_timestamp = self.inner.get_latest_checkpoint().await?.timestamp_ms;

        // gather the requests to fetch in the multi_get_objs.
        let mut requests = vec![record_id];

        // we only want to fetch both the child and the parent if the domain is a subdomain.
        if domain.is_subdomain() {
            requests.push(parent_record_id);
        }

        // fetch both parent (if subdomain) and child records in a single get query.
        // We do this as we do not know if the subdomain is a node or leaf record.
        let domains: Vec<_> = self
            .inner
            .multi_get_objects(requests)
            .await?
            .into_iter()
            .map(|o| sui_types::object::Object::try_from(o).ok())
            .collect();

        // Find the requested object in the list of domains.
        // We need to loop (in an array of maximum size 2), as we cannot guarantee
        // the order of the returned objects.
        let Some(requested_object) = domains
            .iter()
            .find(|o| o.as_ref().is_some_and(|o| o.id() == record_id))
            .and_then(|o| o.clone())
        else {
            return Ok(None);
        };

        let name_record: NameRecord = requested_object.try_into().map_err(IndexerError::from)?;

        // Handle NODE record case.
        if !name_record.is_leaf_record() {
            return if !name_record.is_node_expired(current_timestamp) {
                Ok(name_record.target_address)
            } else {
                Err(IndexerError::NameServiceError(NameServiceError::NameExpired).into())
            };
        }

        // repeat the process for the parent object too.
        let Some(requested_object) = domains
            .iter()
            .find(|o| o.as_ref().is_some_and(|o| o.id() == parent_record_id))
            .and_then(|o| o.clone())
        else {
            return Err(IndexerError::NameServiceError(NameServiceError::NameExpired).into());
        };

        let parent_record: NameRecord = requested_object.try_into().map_err(IndexerError::from)?;

        if parent_record.is_valid_leaf_parent(&name_record)
            && !parent_record.is_node_expired(current_timestamp)
        {
            Ok(name_record.target_address)
        } else {
            Err(IndexerError::NameServiceError(NameServiceError::NameExpired).into())
        }
    }

    async fn resolve_name_service_names(
        &self,
        address: SuiAddress,
        _cursor: Option<ObjectID>,
        _limit: Option<usize>,
    ) -> RpcResult<Page<String, ObjectID>> {
        let reverse_record_id = self
            .name_service_config
            .reverse_record_field_id(address.as_ref());

        let mut result = Page {
            data: vec![],
            next_cursor: None,
            has_next_page: false,
        };

        let Some(field_reverse_record_object) =
            self.inner.get_object(&reverse_record_id, None).await?
        else {
            return Ok(result);
        };

        let domain = field_reverse_record_object
            .to_rust::<Field<SuiAddress, Domain>>()
            .ok_or_else(|| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Malformed Object {reverse_record_id}"
                ))
            })?
            .value;

        let domain_name = domain.to_string();

        // Tries to resolve the name, to verify it is not expired.
        let resolved_address = self
            .resolve_name_service_address(domain_name.clone())
            .await?;

        // If we do not have a resolved address, we do not include the domain in the result.
        if resolved_address.is_none() {
            return Ok(result);
        }

        // We push the domain name to the result and return it.
        result.data.push(domain_name);

        Ok(result)
    }
}

impl SuiRpcModule for IndexerApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc_api::IndexerApiOpenRpc::module_doc()
    }
}
