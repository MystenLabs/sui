// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_graphql::connection::{CursorType, OpaqueCursor};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use move_core_types::annotated_value::{MoveDatatypeLayout, MoveTypeLayout};
use sui_indexer_alt_schema::transactions::StoredTransaction;
use sui_json_rpc_api::{validate_limit, QUERY_MAX_RESULT_LIMIT};
use sui_json_rpc_types::{
    ObjectsPage, Page, SuiObjectDataFilter, SuiObjectResponse, SuiObjectResponseQuery,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::SuiError,
    event::Event,
    signature::GenericSignature,
    transaction::TransactionData,
};

use crate::{
    context::Context,
    data::{
        cursor::JsonCursor,
        objects::{query_objects_with_filters, ObjectCursor, ObjectFilter, TypeFilter},
    },
    error::{internal_error, invalid_params},
};

use super::rpc_module::RpcModule;

pub(crate) type OwnedObjectsPage = Page<SuiObjectResponse, JsonCursor<ObjectCursor>>;

#[open_rpc(namespace = "suix", tag = "Owned Objects API")]
#[rpc(server, namespace = "suix")]
trait OwnedObjectsApi {
    /// Return the list of objects owned by an address.
    /// Note that if the address owns more than `QUERY_MAX_RESULT_LIMIT` objects,
    /// the pagination is not accurate, because previous page may have been updated when
    /// the next page is fetched.
    /// Please use suix_queryObjects if this is a concern.
    #[method(name = "getOwnedObjects")]
    async fn get_owned_objects(
        &self,
        /// the owner's Sui address
        address: SuiAddress,
        /// the objects query criteria.
        query: Option<SuiObjectResponseQuery>,
        /// An optional paging cursor. If provided, the query will start from the next item after the specified cursor. Default to start from the first item if not specified.
        cursor: Option<JsonCursor<ObjectCursor>>,
        /// Max number of items returned per page, default to [QUERY_MAX_RESULT_LIMIT] if not specified.
        limit: Option<usize>,
    ) -> RpcResult<OwnedObjectsPage>;
}

pub(crate) struct OwnedObjects(pub Context);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Unsupported filter: {0}")]
    UnsupportedFilter(String),
}

#[async_trait::async_trait]
impl OwnedObjectsApiServer for OwnedObjects {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<JsonCursor<ObjectCursor>>,
        limit: Option<usize>,
    ) -> RpcResult<OwnedObjectsPage> {
        let filters = query_options_to_filter(&query).map_err(invalid_params)?;
        let cursor = cursor.map(|c| c.into_inner());
        let limit = validate_limit(limit, *QUERY_MAX_RESULT_LIMIT).map_err(invalid_params)?;
        let mut conn = self.0.reader().connect().await.map_err(internal_error)?;
        let (object_ids, next_cursor) =
            query_objects_with_filters(&mut conn, filters, None, cursor, limit)
                .await
                .map_err(internal_error)?;
        Ok(OwnedObjectsPage {
            data: object_ids,
            next_cursor,
            has_next_page: next_cursor.is_some(),
        })
    }
}

impl RpcModule for OwnedObjects {
    fn schema(&self) -> Module {
        OwnedObjectsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

fn query_options_to_filter(query: &Option<SuiObjectResponseQuery>) -> Result<ObjectFilter, Error> {
    let Some(query) = query else {
        return Ok(ObjectFilter::default());
    };
    let Some(filter) = &query.filter else {
        return Ok(ObjectFilter::default());
    };

    let result = match filter {
        SuiObjectDataFilter::MoveModule { package, module } => ObjectFilter {
            type_filter: Some(TypeFilter::Module((*package).into(), module.clone())),
            ..Default::default()
        },
        SuiObjectDataFilter::StructType(tag) => ObjectFilter {
            type_filter: Some(TypeFilter::FullType(tag.clone())),
            ..Default::default()
        },
        SuiObjectDataFilter::AddressOwner(address) => ObjectFilter {
            owner_filter: Some(*address),
            ..Default::default()
        },
        SuiObjectDataFilter::ObjectOwner(object_id) => ObjectFilter {
            owner_filter: Some((*object_id).into()),
            ..Default::default()
        },
        _ => {
            // TODO: support more filters
            return Err(Error::UnsupportedFilter(format!("{:?}", filter)));
        }
    };

    Ok(result)
}

/// Convert the representation of a transaction from the database into the response format,
/// including the fields requested in the `options`.
pub(crate) async fn response(
    ctx: &Context,
    tx: &StoredTransaction,
    options: &SuiTransactionBlockResponseOptions,
) -> Result<SuiTransactionBlockResponse, Error> {
    use Error as E;

    let digest = TransactionDigest::try_from(tx.tx_digest.clone()).map_err(E::Conversion)?;
    let mut response = SuiTransactionBlockResponse::new(digest);

    if options.show_input {
        let data: TransactionData = bcs::from_bytes(&tx.raw_transaction)?;
        let tx_signatures: Vec<GenericSignature> = bcs::from_bytes(&tx.user_signatures)?;
        response.transaction = Some(SuiTransactionBlock {
            data: SuiTransactionBlockData::try_from_with_package_resolver(
                data,
                ctx.package_resolver(),
            )
            .await
            .map_err(E::Resolution)?,
            tx_signatures,
        })
    }

    if options.show_raw_input {
        response.raw_transaction = tx.raw_transaction.clone();
    }

    if options.show_effects {
        let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects)?;
        response.effects = Some(effects.try_into().map_err(E::Conversion)?);
    }

    if options.show_raw_effects {
        response.raw_effects = tx.raw_effects.clone();
    }

    if options.show_events {
        let events: Vec<Event> = bcs::from_bytes(&tx.events)?;
        let mut sui_events = Vec::with_capacity(events.len());

        for (ix, event) in events.into_iter().enumerate() {
            let layout = match ctx
                .package_resolver()
                .type_layout(event.type_.clone().into())
                .await
                .map_err(|e| E::Resolution(e.into()))?
            {
                MoveTypeLayout::Struct(s) => MoveDatatypeLayout::Struct(s),
                MoveTypeLayout::Enum(e) => MoveDatatypeLayout::Enum(e),
                _ => {
                    return Err(E::Resolution(anyhow!(
                        "Event {ix} from {digest} is not a struct or enum: {}",
                        event.type_.to_canonical_string(/* with_prefix */ true)
                    )));
                }
            };

            let sui_event = SuiEvent::try_from(
                event,
                digest,
                ix as u64,
                Some(tx.timestamp_ms as u64),
                layout,
            )
            .map_err(E::Conversion)?;

            sui_events.push(sui_event)
        }

        response.events = Some(SuiTransactionBlockEvents { data: sui_events });
    }

    Ok(response)
}
