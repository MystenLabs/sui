// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use futures::future::OptionFuture;
use move_core_types::annotated_value::{MoveDatatypeLayout, MoveTypeLayout};
use sui_indexer_alt_schema::transactions::{
    BalanceChange, StoredTransaction, StoredTxBalanceChange,
};
use sui_json_rpc_types::{
    BalanceChange as SuiBalanceChange, ObjectChange as SuiObjectChange, SuiEvent,
    SuiTransactionBlock, SuiTransactionBlockData, SuiTransactionBlockEffects,
    SuiTransactionBlockEvents, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::{ObjectDigest, TransactionDigest},
    effects::{IDOperation, ObjectChange, TransactionEffects, TransactionEffectsAPI},
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
        objects::VersionedObjectKey, transactions::TransactionKey,
        tx_balance_changes::TxBalanceChangeKey,
    },
    error::{internal_error, invalid_params, rpc_bail, RpcError},
};

use super::error::Error;

/// Fetch the necessary data from the stores in `ctx` and transform it to build a response for the
/// transaction identified by `digest`, according to the response `options`.
pub(super) async fn transaction(
    ctx: &Context,
    digest: TransactionDigest,
    options: &SuiTransactionBlockResponseOptions,
) -> Result<SuiTransactionBlockResponse, RpcError<Error>> {
    let stored_tx = ctx.loader().load_one(TransactionKey(digest));
    let stored_bc: OptionFuture<_> = options
        .show_balance_changes
        .then(|| ctx.loader().load_one(TxBalanceChangeKey(digest)))
        .into();

    let (stored_tx, stored_bc) = join!(stored_tx, stored_bc);

    let stored_tx = stored_tx
        .context("Failed to fetch transaction from store")?
        .ok_or_else(|| invalid_params(Error::NotFound(digest)))?;

    // Balance changes might not be present because of pruning, in which case we return
    // nothing, even if the changes were requested.
    let stored_bc = match stored_bc
        .transpose()
        .context("Failed to fetch balance changes from store")?
    {
        Some(None) => return Err(invalid_params(Error::PrunedBalanceChanges(digest))),
        Some(changes) => changes,
        None => None,
    };

    let digest = TransactionDigest::try_from(stored_tx.tx_digest.clone())
        .context("Failed to deserialize transaction digest")?;

    let mut response = SuiTransactionBlockResponse::new(digest);

    if options.show_input {
        response.transaction = Some(input(ctx, &stored_tx).await?);
    }

    if options.show_raw_input {
        response.raw_transaction = stored_tx.raw_transaction.clone();
    }

    if options.show_effects {
        response.effects = Some(effects(&stored_tx)?);
    }

    if options.show_raw_effects {
        response.raw_effects = stored_tx.raw_effects.clone();
    }

    if options.show_events {
        response.events = Some(events(ctx, digest, &stored_tx).await?);
    }

    if let Some(changes) = stored_bc {
        response.balance_changes = Some(balance_changes(changes)?);
    }

    if options.show_object_changes {
        response.object_changes = Some(object_changes(ctx, digest, &stored_tx).await?);
    }

    Ok(response)
}

/// Extract a representation of the transaction's input data from the stored form.
async fn input(
    ctx: &Context,
    tx: &StoredTransaction,
) -> Result<SuiTransactionBlock, RpcError<Error>> {
    let data: TransactionData =
        bcs::from_bytes(&tx.raw_transaction).context("Failed to deserialize TransactionData")?;
    let tx_signatures: Vec<GenericSignature> =
        bcs::from_bytes(&tx.user_signatures).context("Failed to deserialize user signatures")?;

    Ok(SuiTransactionBlock {
        data: SuiTransactionBlockData::try_from_with_package_resolver(data, ctx.package_resolver())
            .await
            .context("Failed to resolve types in transaction data")?,
        tx_signatures,
    })
}

/// Extract a representation of the transaction's effects from the stored form.
fn effects(tx: &StoredTransaction) -> Result<SuiTransactionBlockEffects, RpcError<Error>> {
    let effects: TransactionEffects =
        bcs::from_bytes(&tx.raw_effects).context("Failed to deserialize TransactionEffects")?;
    Ok(effects
        .try_into()
        .context("Failed to convert Effects into response")?)
}

/// Extract the transaction's events from its stored form.
async fn events(
    ctx: &Context,
    digest: TransactionDigest,
    tx: &StoredTransaction,
) -> Result<SuiTransactionBlockEvents, RpcError<Error>> {
    let events: Vec<Event> = bcs::from_bytes(&tx.events).context("Failed to deserialize Events")?;
    let mut sui_events = Vec::with_capacity(events.len());

    for (ix, event) in events.into_iter().enumerate() {
        let layout = match ctx
            .package_resolver()
            .type_layout(event.type_.clone().into())
            .await
            .with_context(|| {
                format!(
                    "Failed to resolve layout for {}",
                    event.type_.to_canonical_display(/* with_prefix */ true)
                )
            })? {
            MoveTypeLayout::Struct(s) => MoveDatatypeLayout::Struct(s),
            MoveTypeLayout::Enum(e) => MoveDatatypeLayout::Enum(e),
            _ => rpc_bail!(
                "Event {ix} is not a struct or enum: {}",
                event.type_.to_canonical_string(/* with_prefix */ true)
            ),
        };

        let sui_event = SuiEvent::try_from(
            event,
            digest,
            ix as u64,
            Some(tx.timestamp_ms as u64),
            layout,
        )
        .with_context(|| format!("Failed to convert Event {ix} into response"))?;

        sui_events.push(sui_event)
    }

    Ok(SuiTransactionBlockEvents { data: sui_events })
}

/// Extract the transaction's balance changes from their stored form.
fn balance_changes(
    balance_changes: StoredTxBalanceChange,
) -> Result<Vec<SuiBalanceChange>, RpcError<Error>> {
    let balance_changes: Vec<BalanceChange> = bcs::from_bytes(&balance_changes.balance_changes)
        .context("Failed to deserialize BalanceChanges")?;
    let mut response = Vec::with_capacity(balance_changes.len());

    for BalanceChange::V1 {
        owner,
        coin_type,
        amount,
    } in balance_changes
    {
        let coin_type = TypeTag::from_str(&coin_type)
            .with_context(|| format!("Invalid coin type: {coin_type:?}"))?;

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
async fn object_changes(
    ctx: &Context,
    digest: TransactionDigest,
    tx: &StoredTransaction,
) -> Result<Vec<SuiObjectChange>, RpcError<Error>> {
    let tx_data: TransactionData =
        bcs::from_bytes(&tx.raw_transaction).context("Failed to deserialize TransactionData")?;
    let effects: TransactionEffects =
        bcs::from_bytes(&tx.raw_effects).context("Failed to deserialize TransactionEffects")?;

    let mut keys = vec![];
    let native_changes = effects.object_changes();
    for change in &native_changes {
        let id = change.id;
        if let Some(version) = change.input_version {
            keys.push(VersionedObjectKey(id, version.value()));
        }
        if let Some(version) = change.output_version {
            keys.push(VersionedObjectKey(id, version.value()));
        }
    }

    let objects = ctx
        .loader()
        .load_many(keys)
        .await
        .context("Failed to fetch object contents")?;

    // Fetch and deserialize the contents of an object, based on its object ref. Assumes that all
    // object versions that will be fetched in this way have come from a valid transaction, and
    // have been passed to the data loader in the call above. This means that if they cannot be
    // found, they must have been pruned.
    let fetch_object = |id: ObjectID,
                        v: Option<SequenceNumber>,
                        d: Option<ObjectDigest>|
     -> Result<Option<(Object, ObjectDigest)>, RpcError<Error>> {
        let Some(v) = v else { return Ok(None) };
        let Some(d) = d else { return Ok(None) };

        let v = v.value();

        let stored = objects
            .get(&VersionedObjectKey(id, v))
            .ok_or_else(|| invalid_params(Error::PrunedObject(digest, id, v)))?;

        let bytes = stored
            .serialized_object
            .as_ref()
            .with_context(|| format!("No content for object {id} at version {v}"))?;

        let o = bcs::from_bytes(bytes)
            .with_context(|| format!("Failed to deserialize object {id} at version {v}"))?;

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
            (ID::Created, Some((i, _)), _) => rpc_bail!(
                "Unexpected input version {} for object {object_id} created by transaction {digest}",
                i.version().value(),
            ),

            (ID::Deleted, _, Some((o, _))) => rpc_bail!(
                "Unexpected output version {} for object {object_id} deleted by transaction {digest}",
                o.version().value(),
            ),

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
                    .ok_or_else(|| internal_error!("No type for object {object_id}"))?,
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
                        .ok_or_else(|| internal_error!("No type for object {object_id}"))?,
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
                    .ok_or_else(|| internal_error!("No type for object {object_id}"))?,
                object_id,
                version: o.version(),
                previous_version: i.version(),
                digest: od,
            },

            (ID::None, Some((i, _)), None) => SuiObjectChange::Wrapped {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| internal_error!("No type for object {object_id}"))?,
                object_id,
                version: effects.lamport_version(),
            },

            (ID::Deleted, Some((i, _)), None) => SuiObjectChange::Deleted {
                sender: tx_data.sender(),
                object_type: i
                    .struct_tag()
                    .ok_or_else(|| internal_error!("No type for object {object_id}"))?,
                object_id,
                version: effects.lamport_version(),
            },
        })
    }

    Ok(changes)
}
