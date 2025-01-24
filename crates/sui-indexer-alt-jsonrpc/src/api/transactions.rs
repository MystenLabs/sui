// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use move_core_types::annotated_value::{MoveDatatypeLayout, MoveTypeLayout};
use sui_indexer_alt_schema::transactions::StoredTransaction;
use sui_json_rpc_types::{
    SuiEvent, SuiTransactionBlock, SuiTransactionBlockData, SuiTransactionBlockEvents,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    digests::TransactionDigest, effects::TransactionEffects, error::SuiError, event::Event,
    signature::GenericSignature, transaction::TransactionData,
};

use crate::{
    context::Context,
    error::{internal_error, invalid_params},
};

use super::rpc_module::RpcModule;

#[open_rpc(namespace = "sui", tag = "Transactions API")]
#[rpc(server, namespace = "sui")]
trait TransactionsApi {
    /// Fetch a transaction by its transaction digest.
    #[method(name = "getTransactionBlock")]
    async fn get_transaction_block(
        &self,
        /// The digest of the queried transaction.
        digest: TransactionDigest,
        /// Options controlling the output format.
        options: SuiTransactionBlockResponseOptions,
    ) -> RpcResult<SuiTransactionBlockResponse>;
}

pub(crate) struct Transactions(pub Context);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Transaction not found: {0}")]
    NotFound(TransactionDigest),

    #[error("Error converting to response: {0}")]
    Conversion(SuiError),

    #[error("Error resolving type information: {0}")]
    Resolution(anyhow::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] bcs::Error),
}

#[async_trait::async_trait]
impl TransactionsApiServer for Transactions {
    async fn get_transaction_block(
        &self,
        digest: TransactionDigest,
        options: SuiTransactionBlockResponseOptions,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        let Self(ctx) = self;
        let Some(stored) = ctx
            .loader()
            .load_one(digest)
            .await
            .map_err(internal_error)?
        else {
            return Err(invalid_params(Error::NotFound(digest)));
        };

        response(ctx, &stored, &options)
            .await
            .map_err(internal_error)
    }
}

impl RpcModule for Transactions {
    fn schema(&self) -> Module {
        TransactionsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
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
