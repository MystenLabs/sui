// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::anyhow;
use futures::future::OptionFuture;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use move_core_types::annotated_value::{MoveDatatypeLayout, MoveTypeLayout};
use sui_indexer_alt_schema::transactions::{
    BalanceChange, StoredTransaction, StoredTxBalanceChange,
};
use sui_json_rpc_types::{
    BalanceChange as SuiBalanceChange, ObjectChange as SuiObjectChange, SuiEvent,
    SuiTransactionBlock, SuiTransactionBlockData, SuiTransactionBlockEffects,
    SuiTransactionBlockEvents, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::{ObjectDigest, TransactionDigest},
    effects::{IDOperation, ObjectChange, TransactionEffects, TransactionEffectsAPI},
    error::SuiError,
    event::Event,
    object::Object,
    signature::GenericSignature,
    transaction::{TransactionData, TransactionDataAPI},
    TypeTag,
};
use tokio::join;

use crate::{
    context::Context,
    data::{
        objects::ObjectVersionKey, transactions::TransactionKey,
        tx_balance_changes::TxBalanceChangeKey,
    },
    error::{internal_error, invalid_params, pruned, rpc_bail},
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
        use Error as E;

        let Self(ctx) = self;

        let transaction = ctx.loader().load_one(TransactionKey(digest));
        let balance_changes: OptionFuture<_> = options
            .show_balance_changes
            .then(|| ctx.loader().load_one(TxBalanceChangeKey(digest)))
            .into();

        let (transaction, balance_changes) = join!(transaction, balance_changes);

        let transaction = transaction
            .map_err(internal_error)?
            .ok_or_else(|| invalid_params(E::NotFound(digest)))?;

        // Balance changes might not be present because of pruning, in which case we return
        // nothing, even if the changes were requested.
        let balance_changes = match balance_changes.transpose().map_err(internal_error)? {
            Some(None) => rpc_bail!(pruned("balance changes for transaction {digest}")),
            Some(changes) => changes,
            None => None,
        };

        let digest = TransactionDigest::try_from(transaction.tx_digest.clone())
            .map_err(E::Conversion)
            .map_err(internal_error)?;

        let mut response = SuiTransactionBlockResponse::new(digest);

        if options.show_input {
            response.transaction = Some(
                input_response(ctx, &transaction)
                    .await
                    .map_err(internal_error)?,
            );
        }

        if options.show_raw_input {
            response.raw_transaction = transaction.raw_transaction.clone();
        }

        if options.show_effects {
            response.effects = Some(effects_response(&transaction).map_err(internal_error)?);
        }

        if options.show_raw_effects {
            response.raw_effects = transaction.raw_effects.clone();
        }

        if options.show_events {
            response.events = Some(
                events_response(ctx, digest, &transaction)
                    .await
                    .map_err(internal_error)?,
            );
        }

        if let Some(balance_changes) = balance_changes {
            response.balance_changes =
                Some(balance_changes_response(balance_changes).map_err(internal_error)?);
        }

        if options.show_object_changes {
            response.object_changes =
                Some(object_changes_response(ctx, digest, &transaction).await?);
        }

        Ok(response)
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

/// Extract a representation of the transaction's input data from the stored form.
async fn input_response(
    ctx: &Context,
    tx: &StoredTransaction,
) -> Result<SuiTransactionBlock, Error> {
    let data: TransactionData = bcs::from_bytes(&tx.raw_transaction)?;
    let tx_signatures: Vec<GenericSignature> = bcs::from_bytes(&tx.user_signatures)?;

    Ok(SuiTransactionBlock {
        data: SuiTransactionBlockData::try_from_with_package_resolver(data, ctx.package_resolver())
            .await
            .map_err(Error::Resolution)?,
        tx_signatures,
    })
}

/// Extract a representation of the transaction's effects from the stored form.
fn effects_response(tx: &StoredTransaction) -> Result<SuiTransactionBlockEffects, Error> {
    let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects)?;
    effects.try_into().map_err(Error::Conversion)
}

/// Extract the transaction's events from its stored form.
async fn events_response(
    ctx: &Context,
    digest: TransactionDigest,
    tx: &StoredTransaction,
) -> Result<SuiTransactionBlockEvents, Error> {
    use Error as E;

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

    Ok(SuiTransactionBlockEvents { data: sui_events })
}

/// Extract the transaction's balance changes from their stored form.
fn balance_changes_response(
    balance_changes: StoredTxBalanceChange,
) -> Result<Vec<SuiBalanceChange>, Error> {
    let balance_changes: Vec<BalanceChange> = bcs::from_bytes(&balance_changes.balance_changes)?;
    let mut response = Vec::with_capacity(balance_changes.len());

    for BalanceChange::V1 {
        owner,
        coin_type,
        amount,
    } in balance_changes
    {
        let coin_type = TypeTag::from_str(&coin_type).map_err(Error::Resolution)?;
        response.push(SuiBalanceChange {
            owner,
            coin_type,
            amount,
        });
    }

    Ok(response)
}

/// Extract the transaction's object changes. Object IDs and versions are fetched from the stored
/// transaction, and the object contents are fetched separately by a data loader.
async fn object_changes_response(
    ctx: &Context,
    digest: TransactionDigest,
    tx: &StoredTransaction,
) -> RpcResult<Vec<SuiObjectChange>> {
    let tx_data: TransactionData = bcs::from_bytes(&tx.raw_transaction)
        .map_err(Error::from)
        .map_err(internal_error)?;

    let effects: TransactionEffects = bcs::from_bytes(&tx.raw_effects)
        .map_err(Error::from)
        .map_err(internal_error)?;

    let mut keys = vec![];
    let native_changes = effects.object_changes();
    for change in &native_changes {
        let id = change.id;
        if let Some(version) = change.input_version {
            keys.push(ObjectVersionKey(id, version.value()));
        }
        if let Some(version) = change.output_version {
            keys.push(ObjectVersionKey(id, version.value()));
        }
    }

    let objects = ctx
        .loader()
        .load_many(keys)
        .await
        .map_err(|e| internal_error(e.to_string()))?;

    // Fetch and deserialize the contents of an object, based on its object ref. Assumes that all
    // object versions that will be fetched in this way have come from a valid transaction, and
    // have been passed to the data loader in the call above. This means that if they cannot be
    // found, they must have been pruned.
    let fetch_object = |id: ObjectID,
                        v: Option<SequenceNumber>,
                        d: Option<ObjectDigest>|
     -> RpcResult<Option<(Object, ObjectDigest)>> {
        let Some(v) = v else { return Ok(None) };
        let Some(d) = d else { return Ok(None) };

        let v = v.value();

        let stored = objects
            .get(&ObjectVersionKey(id, v))
            .ok_or_else(|| pruned(format!("Object {id} at version {v}")))?;

        let bytes = stored
            .serialized_object
            .as_ref()
            .ok_or_else(|| internal_error("No content for object {id} at version {v}"))?;

        let o = bcs::from_bytes(bytes)
            .map_err(Error::from)
            .map_err(internal_error)?;

        Ok(Some((o, d)))
    };

    let mut changes = Vec::with_capacity(native_changes.len());

    for change in native_changes {
        let &ObjectChange {
            id: object_id,
            id_operation,
            input_version,
            input_digest,
            output_version,
            output_digest,
            ..
        } = &change;

        let input = fetch_object(object_id, input_version, input_digest)?;
        let output = fetch_object(object_id, output_version, output_digest)?;

        use IDOperation as ID;
        changes.push(match (id_operation, input, output) {
            (ID::Created, Some((i, _)), _) => rpc_bail!(internal_error(
                "Unexpected input version {} for object {object_id} created by transaction {digest}",
                i.version().value(),
            )),

            (ID::Deleted, _, Some((o, _))) => rpc_bail!(internal_error(
                "Unexpected output version {} for object {object_id} deleted by transaction {digest}",
                o.version().value(),
            )),

            // The following cases don't end up in the output: created and wrapped objects,
            // unwrapped objects (and by extension, unwrapped and deleted objects), system package
            // upgrades (which happen in place).
            (ID::Created, _, None) => continue,
            (ID::None, None, _) => continue,
            (ID::None, _, Some((o, _))) if o.is_package() => continue,
            (ID::Deleted, None, _) => continue,

            (ID::Created, _, Some((o, d))) if o.is_package() => SuiObjectChange::Published {
                package_id: object_id,
                version: o.version(),
                digest: d,
                modules: o
                    .data
                    .try_as_package()
                    .unwrap() // SAFETY: Match guard checks that the object is a package.
                    .serialized_module_map()
                    .keys()
                    .cloned()
                    .collect(),
            },

            (ID::Created, _, Some((o, d))) => SuiObjectChange::Created {
                sender: tx_data.sender(),
                owner: o.owner().clone(),
                object_type: o
                    .struct_tag()
                    .ok_or_else(|| internal_error(format!("No type for object {object_id}")))?,
                object_id,
                version: o.version(),
                digest: d,
            },

            (ID::None, Some((i, _)), Some((o, od))) if i.owner() != o.owner() => {
                SuiObjectChange::Transferred {
                    sender: tx_data.sender(),
                    recipient: o.owner().clone(),
                    object_type: o
                        .struct_tag()
                        .ok_or_else(|| internal_error(format!("No type for object {object_id}")))?,
                    object_id,
                    version: o.version(),
                    digest: od,
                }
            }

            (ID::None, Some((i, _)), Some((o, od))) => SuiObjectChange::Mutated {
                sender: tx_data.sender(),
                owner: o.owner().clone(),
                object_type: o
                    .struct_tag()
                    .ok_or_else(|| internal_error(format!("No type for object {object_id}")))?,
                object_id,
                version: o.version(),
                previous_version: i.version(),
                digest: od,
            },

            (ID::None, Some((i, _)), None) => SuiObjectChange::Wrapped {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| internal_error(format!("No type for object {object_id}")))?,
                object_id,
                version: effects.lamport_version(),
            },

            (ID::Deleted, Some((i, _)), None) => SuiObjectChange::Deleted {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| internal_error(format!("No type for object {object_id}")))?,
                object_id,
                version: effects.lamport_version(),
            },
        })
    }

    Ok(changes)
}
