// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Context as _;
use move_core_types::annotated_value::MoveDatatypeLayout;
use move_core_types::annotated_value::MoveTypeLayout;
use sui_json_rpc_types::BalanceChange as SuiBalanceChange;
use sui_json_rpc_types::DryRunTransactionBlockResponse;
use sui_json_rpc_types::ObjectChange as SuiObjectChange;
use sui_json_rpc_types::SuiEvent;
use sui_json_rpc_types::SuiTransactionBlock;
use sui_json_rpc_types::SuiTransactionBlockData;
use sui_json_rpc_types::SuiTransactionBlockEffects;
use sui_json_rpc_types::SuiTransactionBlockEvents;
use sui_json_rpc_types::SuiTransactionBlockResponse;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::ObjectChange;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionDataAPI;

use crate::api::to_sui_object_change;
use crate::context::Context;
use crate::error::RpcError;

use super::Error;

pub(super) async fn transaction(
    ctx: &Context,
    tx_data: TransactionData,
    tx_signatures: Vec<GenericSignature>,
    executed_tx: &proto::ExecutedTransaction,
    options: &SuiTransactionBlockResponseOptions,
) -> Result<SuiTransactionBlockResponse, RpcError<Error>> {
    let tx_digest = tx_data.digest();
    let mut result = SuiTransactionBlockResponse::new(tx_digest);
    result.checkpoint = executed_tx.checkpoint;
    result.timestamp_ms = executed_tx
        .timestamp
        .and_then(|ts| sui_rpc::proto::proto_to_timestamp_ms(ts).ok());

    if options.show_input {
        result.transaction = Some(input(ctx, tx_data.clone(), tx_signatures).await?);
    }

    if options.show_raw_input {
        result.raw_transaction = raw_input(&tx_data)?;
    }

    if options.show_raw_effects {
        result.raw_effects = raw_effects(executed_tx)?;
    }

    if options.show_effects || options.show_object_changes {
        let effects = deserialize_effects(executed_tx)?;

        if options.show_effects {
            result.effects = Some(effects_response(&effects)?);
        }

        if options.show_object_changes {
            result.object_changes = Some(object_changes(tx_data.sender(), &effects, executed_tx)?);
        }
    }

    if options.show_events {
        result.events = Some(events(ctx, tx_digest, executed_tx).await?);
    }

    if options.show_balance_changes {
        result.balance_changes = Some(balance_changes(executed_tx)?);
    }

    Ok(result)
}

pub(super) async fn dry_run(
    ctx: &Context,
    tx_data: TransactionData,
    executed_tx: &proto::ExecutedTransaction,
    suggested_gas_price: Option<u64>,
) -> Result<DryRunTransactionBlockResponse, RpcError<Error>> {
    let effects = deserialize_effects(executed_tx)?;
    let tx_digest = tx_data.digest();

    Ok(DryRunTransactionBlockResponse {
        effects: effects_response(&effects)?,
        events: events(ctx, tx_digest, executed_tx).await?,
        object_changes: object_changes(tx_data.sender(), &effects, executed_tx)?,
        balance_changes: balance_changes(executed_tx)?,
        input: input(ctx, tx_data, vec![]).await?.data,
        execution_error_source: None,
        suggested_gas_price,
    })
}

/// Build a representation of the transaction's input data for the response.
async fn input(
    ctx: &Context,
    tx_data: TransactionData,
    tx_signatures: Vec<GenericSignature>,
) -> Result<SuiTransactionBlock, RpcError<Error>> {
    let data =
        SuiTransactionBlockData::try_from_with_package_resolver(tx_data, ctx.package_resolver())
            .await
            .context("Failed to convert transaction data")?;
    Ok(SuiTransactionBlock {
        data,
        tx_signatures,
    })
}

/// Serialize transaction data to raw BCS bytes.
fn raw_input(tx_data: &TransactionData) -> Result<Vec<u8>, RpcError<Error>> {
    Ok(bcs::to_bytes(tx_data).context("Failed to serialize transaction")?)
}

/// Extract the raw effects BCS bytes from the gRPC response.
fn raw_effects(executed_tx: &proto::ExecutedTransaction) -> Result<Vec<u8>, RpcError<Error>> {
    let effects_bcs = executed_tx
        .effects
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .context("Missing effects.bcs in gRPC response")?;
    Ok(effects_bcs.value().to_vec())
}

/// Deserialize events from the gRPC response and resolve their layouts.
async fn events(
    ctx: &Context,
    tx_digest: TransactionDigest,
    executed_tx: &proto::ExecutedTransaction,
) -> Result<SuiTransactionBlockEvents, RpcError<Error>> {
    let events_bcs = executed_tx.events.as_ref().and_then(|e| e.bcs.as_ref());

    let events: Vec<Event> = match events_bcs {
        Some(bcs) => bcs
            .deserialize()
            .context("Failed to deserialize event BCS from gRPC response")?,
        None => vec![],
    };

    let mut sui_events = Vec::with_capacity(events.len());
    for (ix, event) in events.into_iter().enumerate() {
        let layout = match ctx
            .package_resolver()
            .type_layout(event.type_.clone().into())
            .await
            .context("Failed to resolve event type layout")?
        {
            MoveTypeLayout::Struct(s) => MoveDatatypeLayout::Struct(s),
            MoveTypeLayout::Enum(e) => MoveDatatypeLayout::Enum(e),
            _ => {
                return Err(anyhow::anyhow!(
                    "Event {ix} is not a struct or enum: {}",
                    event.type_.to_canonical_string(true)
                )
                .into());
            }
        };
        sui_events.push(
            SuiEvent::try_from(event, tx_digest, ix as u64, None, layout)
                .context("Failed to convert event into JSON-RPC response type")?,
        );
    }

    Ok(SuiTransactionBlockEvents { data: sui_events })
}

/// Convert balance changes from the gRPC response.
fn balance_changes(
    executed_tx: &proto::ExecutedTransaction,
) -> Result<Vec<SuiBalanceChange>, RpcError<Error>> {
    let mut changes = Vec::with_capacity(executed_tx.balance_changes.len());
    for bc in &executed_tx.balance_changes {
        let addr: SuiAddress = bc
            .address
            .as_ref()
            .context("Missing address in balance change")?
            .parse()
            .context("Invalid owner address in balance change")?;
        let owner = Owner::AddressOwner(addr);

        let coin_type = TypeTag::from_str(
            bc.coin_type
                .as_ref()
                .context("Missing coin_type in balance change")?,
        )
        .context("Invalid coin type in balance change")?;

        let amount: i128 = bc
            .amount
            .as_ref()
            .context("Missing amount in balance change")?
            .parse()
            .context("Invalid balance change amount")?;

        changes.push(SuiBalanceChange {
            owner,
            coin_type,
            amount,
        });
    }

    Ok(changes)
}

/// Build object changes by correlating effects with the output objects from the gRPC response.
fn object_changes(
    sender: SuiAddress,
    effects: &TransactionEffects,
    executed_tx: &proto::ExecutedTransaction,
) -> Result<Vec<SuiObjectChange>, RpcError<Error>> {
    let native_changes = effects.object_changes();

    // Build a map of (ObjectID, version) -> Object from the proto objects. Objects that are
    // Wrapped or Deleted will not have BCS content and are skipped here.
    let mut objects: HashMap<(ObjectID, u64), Object> = HashMap::new();
    if let Some(object_set) = &executed_tx.objects {
        for proto_obj in &object_set.objects {
            if let Some(bcs) = &proto_obj.bcs {
                let obj: Object = bcs
                    .deserialize()
                    .context("Failed to deserialize changed object from gRPC response")?;
                objects.insert((obj.id(), obj.version().value()), obj);
            }
        }
    }

    let fetch_object = |id: ObjectID,
                        v: Option<SequenceNumber>,
                        d: Option<ObjectDigest>|
     -> Result<Option<(Object, ObjectDigest)>, RpcError<Error>> {
        let Some(v) = v else { return Ok(None) };
        let Some(d) = d else { return Ok(None) };
        let key = (id, v.value());
        match objects.get(&key) {
            Some(o) => Ok(Some((o.clone(), d))),
            None => Err(crate::error::internal_error!(
                "Object {id} at version {} referenced in effects but missing BCS in gRPC response",
                v.value(),
            )),
        }
    };

    let mut changes = Vec::with_capacity(native_changes.len());
    for change in &native_changes {
        let &ObjectChange {
            id: object_id,
            id_operation,
            input_version,
            input_digest,
            output_version,
            output_digest,
            ..
        } = change;

        let input = fetch_object(object_id, input_version, input_digest)?;
        let output = fetch_object(object_id, output_version, output_digest)?;

        changes.extend(to_sui_object_change(
            sender,
            object_id,
            id_operation,
            input,
            output,
            effects.lamport_version(),
        )?);
    }

    Ok(changes)
}

/// Deserialize `TransactionEffects` from the BCS field in the gRPC response.
fn deserialize_effects(
    executed_tx: &proto::ExecutedTransaction,
) -> Result<TransactionEffects, RpcError<Error>> {
    let effects_bcs = executed_tx
        .effects
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .context("Missing effects.bcs in gRPC response")?;
    Ok(effects_bcs
        .deserialize()
        .context("Failed to deserialize effects from gRPC response")?)
}

fn effects_response(
    effects: &TransactionEffects,
) -> Result<SuiTransactionBlockEffects, RpcError<Error>> {
    Ok(effects
        .clone()
        .try_into()
        .context("Failed to convert effects into JSON-RPC response type")?)
}
