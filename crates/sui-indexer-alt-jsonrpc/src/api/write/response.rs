// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;

use move_core_types::annotated_value::MoveDatatypeLayout;
use move_core_types::annotated_value::MoveTypeLayout;
use sui_json_rpc_types::BalanceChange as SuiBalanceChange;
use sui_json_rpc_types::ObjectChange as SuiObjectChange;
use sui_json_rpc_types::SuiEvent;
use sui_json_rpc_types::SuiTransactionBlock;
use sui_json_rpc_types::SuiTransactionBlockData;
use sui_json_rpc_types::SuiTransactionBlockEvents;
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

use crate::api::to_sui_object_change;
use crate::context::Context;

/// Build a representation of the transaction's input data for the response.
pub(super) async fn input(
    ctx: &Context,
    tx_data: TransactionData,
    signatures: Vec<GenericSignature>,
) -> anyhow::Result<SuiTransactionBlock> {
    let data =
        SuiTransactionBlockData::try_from_with_package_resolver(tx_data, ctx.package_resolver())
            .await?;
    Ok(SuiTransactionBlock {
        data,
        tx_signatures: signatures,
    })
}

/// Serialize transaction data to raw BCS bytes.
pub(super) fn raw_input(tx_data: &TransactionData) -> anyhow::Result<Vec<u8>> {
    Ok(bcs::to_bytes(tx_data)?)
}

/// Extract the raw effects BCS bytes from the gRPC response.
pub(super) fn raw_effects(executed_tx: &proto::ExecutedTransaction) -> anyhow::Result<Vec<u8>> {
    let effects_bcs = executed_tx
        .effects
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Missing effects.bcs in gRPC response"))?;
    Ok(effects_bcs.value().to_vec())
}

/// Deserialize events from the gRPC response and resolve their layouts.
pub(super) async fn events(
    ctx: &Context,
    tx_digest: TransactionDigest,
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<SuiTransactionBlockEvents> {
    let events_bcs = executed_tx.events.as_ref().and_then(|e| e.bcs.as_ref());

    let events: Vec<Event> = match events_bcs {
        Some(bcs) => bcs.deserialize()?,
        None => vec![],
    };

    let mut sui_events = Vec::with_capacity(events.len());
    for (ix, event) in events.into_iter().enumerate() {
        let layout = match ctx
            .package_resolver()
            .type_layout(event.type_.clone().into())
            .await?
        {
            MoveTypeLayout::Struct(s) => MoveDatatypeLayout::Struct(s),
            MoveTypeLayout::Enum(e) => MoveDatatypeLayout::Enum(e),
            _ => anyhow::bail!(
                "Event {ix} is not a struct or enum: {}",
                event.type_.to_canonical_string(true)
            ),
        };
        sui_events.push(SuiEvent::try_from(
            event, tx_digest, ix as u64, None, layout,
        )?);
    }

    Ok(SuiTransactionBlockEvents { data: sui_events })
}

/// Convert balance changes from the gRPC response.
pub(super) fn balance_changes(
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<Vec<SuiBalanceChange>> {
    executed_tx
        .balance_changes
        .iter()
        .map(|bc| {
            let addr: SuiAddress = bc
                .address
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing address in balance change"))?
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid owner address: {e}"))?;
            let owner = Owner::AddressOwner(addr);
            let coin_type = TypeTag::from_str(
                bc.coin_type
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing coin_type in balance change"))?,
            )
            .map_err(|e| anyhow::anyhow!("Invalid coin type: {e}"))?;
            let amount: i128 = bc
                .amount
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing amount in balance change"))?
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid balance change amount: {e}"))?;

            Ok(SuiBalanceChange {
                owner,
                coin_type,
                amount,
            })
        })
        .collect()
}

/// Build object changes by correlating effects with the output objects from the gRPC response.
pub(super) fn object_changes(
    sender: SuiAddress,
    effects: &TransactionEffects,
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<Vec<SuiObjectChange>> {
    let native_changes = effects.object_changes();

    // Build a map of (ObjectID, version) -> Object from the proto objects. Objects that are
    // Wrapped or Deleted will not have BCS content and are skipped here.
    let mut objects: HashMap<(ObjectID, u64), Object> = HashMap::new();
    if let Some(object_set) = &executed_tx.objects {
        for proto_obj in &object_set.objects {
            if let Some(bcs) = &proto_obj.bcs {
                let obj: Object = bcs.deserialize()?;
                objects.insert((obj.id(), obj.version().value()), obj);
            }
        }
    }

    let fetch_object = |id: ObjectID,
                        v: Option<SequenceNumber>,
                        d: Option<ObjectDigest>|
     -> anyhow::Result<Option<(Object, ObjectDigest)>> {
        let Some(v) = v else { return Ok(None) };
        let Some(d) = d else { return Ok(None) };
        let key = (id, v.value());
        match objects.get(&key) {
            Some(o) => Ok(Some((o.clone(), d))),
            None => anyhow::bail!(
                "Object {id} at version {} referenced in effects but missing BCS \
                 in gRPC response",
                v.value(),
            ),
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

        if let Some(sui_change) = to_sui_object_change(
            sender,
            object_id,
            id_operation,
            input,
            output,
            effects.lamport_version(),
        )? {
            changes.push(sui_change);
        }
    }

    Ok(changes)
}

/// Deserialize `TransactionEffects` from the BCS field in the gRPC response.
pub(super) fn deserialize_effects(
    executed_tx: &proto::ExecutedTransaction,
) -> anyhow::Result<TransactionEffects> {
    let effects_bcs = executed_tx
        .effects
        .as_ref()
        .and_then(|e| e.bcs.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Missing effects.bcs in gRPC response"))?;
    Ok(effects_bcs.deserialize()?)
}
