// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use jsonrpsee::RpcModule;
use jsonrpsee::server::Server;
use jsonrpsee::types::ErrorObjectOwned;
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::normalized::RcPool;
use move_bytecode_utils::module_cache::ModuleCache;
use move_core_types::language_storage::StructTag;
use serde_json::Value;
use simulacrum::store::SimulatorStore;
use sui_json_rpc_types::{SuiMoveNormalizedFunction, SuiMoveNormalizedModule, SuiMoveStruct};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;
use sui_types::object::{Data, Object, Owner};
use sui_types::storage::ObjectStore;
use tokio::sync::Mutex;

use crate::ForkedNode;
use crate::store::ForkedStore;

fn internal_error(msg: impl std::fmt::Display) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32603, format!("{msg}"), None::<()>)
}

/// Build a `SuiObjectResponse`-compatible JSON value from an `Object`.
fn build_object_response(obj: &Object) -> Value {
    let owner_json = serde_json::to_value(&obj.owner).unwrap_or(Value::Null);
    let (type_str, bcs_json) = match &obj.data {
        Data::Move(move_obj) => {
            let type_str = move_obj.type_().to_string();
            let bcs_bytes = BASE64_STANDARD.encode(move_obj.contents());
            let bcs_json = serde_json::json!({
                "dataType": "moveObject",
                "type": type_str,
                "hasPublicTransfer": move_obj.has_public_transfer(),
                "bcsBytes": bcs_bytes,
            });
            (type_str, bcs_json)
        }
        Data::Package(pkg) => {
            let module_map: HashMap<String, String> = pkg
                .serialized_module_map()
                .iter()
                .map(|(k, v)| (k.clone(), BASE64_STANDARD.encode(v)))
                .collect();
            (
                "package".to_string(),
                serde_json::json!({
                    "dataType": "package",
                    "id": obj.id().to_string(),
                    "moduleMap": module_map,
                }),
            )
        }
    };
    serde_json::json!({
        "data": {
            "objectId": obj.id().to_string(),
            "version": obj.version().value().to_string(),
            "digest": obj.digest().to_string(),
            "type": type_str,
            "owner": owner_json,
            "previousTransaction": obj.previous_transaction.to_string(),
            "storageRebate": obj.storage_rebate.to_string(),
            "bcs": bcs_json,
        }
    })
}

/// Build a human-readable effects JSON from `TransactionEffects`.
/// Object type strings are resolved by looking up objects in `store`.
fn build_effects_json(
    effects: &impl TransactionEffectsAPI,
    store: &ForkedStore,
) -> Value {
    let status_json = match effects.status() {
        ExecutionStatus::Success => serde_json::json!({ "status": "success" }),
        ExecutionStatus::Failure { error, command } => serde_json::json!({
            "status": "failure",
            "error": format!("{error:?}"),
            "failedCommand": command,
        }),
    };

    let gas = effects.gas_cost_summary();
    let gas_used = serde_json::json!({
        "computationCost": gas.computation_cost.to_string(),
        "storageCost": gas.storage_cost.to_string(),
        "storageRebate": gas.storage_rebate.to_string(),
        "nonRefundableStorageFee": gas.non_refundable_storage_fee.to_string(),
    });

    let resolve_type =
        |id: sui_types::base_types::ObjectID, version: SequenceNumber| -> String {
            store
                .get_object_by_key(&id, version)
                .or_else(|| ObjectStore::get_object(store, &id))
                .and_then(|obj| obj.data.type_().map(|t| t.to_string()))
                .unwrap_or_default()
        };

    let obj_with_owner = |(obj_ref, owner): (
        (
            sui_types::base_types::ObjectID,
            SequenceNumber,
            sui_types::digests::ObjectDigest,
        ),
        Owner,
    )| {
        let (id, version, digest) = obj_ref;
        serde_json::json!({
            "objectId": id.to_string(),
            "version": version.value().to_string(),
            "digest": digest.to_string(),
            "type": resolve_type(id, version),
            "owner": serde_json::to_value(&owner).unwrap_or(Value::Null),
        })
    };

    let obj_ref_only =
        |(id, version, digest): (
            sui_types::base_types::ObjectID,
            SequenceNumber,
            sui_types::digests::ObjectDigest,
        )| {
            serde_json::json!({
                "objectId": id.to_string(),
                "version": version.value().to_string(),
                "digest": digest.to_string(),
            })
        };

    serde_json::json!({
        "transactionDigest": effects.transaction_digest().to_string(),
        "executedEpoch": effects.executed_epoch().to_string(),
        "status": status_json,
        "gasUsed": gas_used,
        "created":   effects.created().into_iter().map(obj_with_owner).collect::<Vec<_>>(),
        "mutated":   effects.mutated().into_iter().map(obj_with_owner).collect::<Vec<_>>(),
        "unwrapped": effects.unwrapped().into_iter().map(obj_with_owner).collect::<Vec<_>>(),
        "deleted":   effects.deleted().into_iter().map(obj_ref_only).collect::<Vec<_>>(),
        "wrapped":   effects.wrapped().into_iter().map(obj_ref_only).collect::<Vec<_>>(),
    })
}

/// Build a JSON array of events from `TransactionEvents`.
fn build_events_json(
    events: &sui_types::effects::TransactionEvents,
    tx_digest: &TransactionDigest,
) -> Value {
    let list: Vec<Value> = events
        .data
        .iter()
        .enumerate()
        .map(|(i, event)| {
            serde_json::json!({
                "id": {
                    "txDigest": tx_digest.to_string(),
                    "eventSeq": i.to_string(),
                },
                "packageId": event.package_id.to_string(),
                "transactionModule": event.transaction_module.to_string(),
                "sender": event.sender.to_string(),
                "type": event.type_.to_string(),
                "bcsEncoded": BASE64_STANDARD.encode(&event.contents),
            })
        })
        .collect();
    Value::Array(list)
}

pub async fn serve(node: Arc<Mutex<ForkedNode>>, port: u16) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let server = Server::builder().build(addr).await?;
    let mut module = RpcModule::new(());

    // --- sui_getChainIdentifier ---
    {
        let node = node.clone();
        module.register_async_method("sui_getChainIdentifier", move |_, _, _| {
            let node = node.clone();
            async move {
                let n = node.lock().await;
                Ok::<Value, ErrorObjectOwned>(Value::String(n.chain_id.clone()))
            }
        })?;
    }

    // --- sui_getReferenceGasPrice ---
    {
        let node = node.clone();
        module.register_async_method("sui_getReferenceGasPrice", move |_, _, _| {
            let node = node.clone();
            async move {
                let n = node.lock().await;
                Ok::<Value, ErrorObjectOwned>(Value::String(
                    n.reference_gas_price().to_string(),
                ))
            }
        })?;
    }

    // --- sui_getLatestCheckpointSequenceNumber ---
    {
        let node = node.clone();
        module.register_async_method(
            "sui_getLatestCheckpointSequenceNumber",
            move |_, _, _| {
                let node = node.clone();
                async move {
                    let n = node.lock().await;
                    Ok::<Value, ErrorObjectOwned>(Value::String(
                        n.fork_checkpoint.to_string(),
                    ))
                }
            },
        )?;
    }

    // --- sui_getObject ---
    {
        let node = node.clone();
        module.register_async_method("sui_getObject", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let object_id: sui_types::base_types::ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let n = node.lock().await;
                match n.get_object(&object_id) {
                    Some(obj) => Ok::<Value, ErrorObjectOwned>(build_object_response(&obj)),
                    None => Ok(serde_json::json!({ "error": { "code": "objectNotFound" } })),
                }
            }
        })?;
    }

    // --- sui_multiGetObjects ---
    {
        let node = node.clone();
        module.register_async_method("sui_multiGetObjects", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let ids: Vec<String> = p.next().map_err(internal_error)?;
                let n = node.lock().await;
                let results: Vec<Value> = ids
                    .iter()
                    .map(|id_str| {
                        let Ok(object_id) = id_str.parse::<sui_types::base_types::ObjectID>()
                        else {
                            return serde_json::json!({ "error": { "code": "invalidObjectId" } });
                        };
                        match n.get_object(&object_id) {
                            Some(obj) => build_object_response(&obj),
                            None => {
                                serde_json::json!({ "error": { "code": "objectNotFound" } })
                            }
                        }
                    })
                    .collect();
                Ok::<Value, ErrorObjectOwned>(Value::Array(results))
            }
        })?;
    }

    // --- sui_getOwnedObjects ---
    {
        let node = node.clone();
        module.register_async_method("sui_getOwnedObjects", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let n = node.lock().await;
                let objects: Vec<Value> = n
                    .store
                    .owned_objects(address)
                    .map(|obj| serde_json::json!({ "data": build_object_response(&obj)["data"] }))
                    .collect();
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "data": objects,
                    "nextCursor": Value::Null,
                    "hasNextPage": false,
                }))
            }
        })?;
    }

    // --- sui_getProtocolConfig ---
    {
        let node = node.clone();
        module.register_async_method("sui_getProtocolConfig", move |_, _, _| {
            let node = node.clone();
            async move {
                let n = node.lock().await;
                let version = n.epoch_state.protocol_config().version.as_u64();
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "protocolVersion": version.to_string(),
                    "featureFlags": {},
                    "attributes": {},
                    "maxSupportedProtocolVersion": version.to_string(),
                    "minSupportedProtocolVersion": "1",
                }))
            }
        })?;
    }

    // --- sui_executeTransactionBlock ---
    {
        let node = node.clone();
        module.register_async_method(
            "sui_executeTransactionBlock",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let tx_bytes_b64: String = p.next().map_err(internal_error)?;
                    let tx_bytes = BASE64_STANDARD
                        .decode(&tx_bytes_b64)
                        .map_err(|e| internal_error(format!("base64 decode: {e}")))?;
                    let tx_data: sui_types::transaction::TransactionData =
                        bcs::from_bytes(&tx_bytes)
                            .map_err(|e| internal_error(format!("bcs decode error: {e}")))?;
                    let mut n = node.lock().await;
                    let (effects, events) = n
                        .execute_transaction(tx_data)
                        .map_err(internal_error)?;
                    let digest = effects.transaction_digest().to_string();
                    let effects_json = build_effects_json(&effects, &n.store);
                    let events_json = build_events_json(&events, effects.transaction_digest());
                    let effects_bcs = bcs::to_bytes(&effects)
                        .map_err(|e| internal_error(format!("effects serialization: {e}")))?;
                    let events_bcs = bcs::to_bytes(&events)
                        .map_err(|e| internal_error(format!("events serialization: {e}")))?;
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "digest": digest,
                        "effects": effects_json,
                        "events": events_json,
                        "rawEffects": BASE64_STANDARD.encode(&effects_bcs),
                        "rawEvents": BASE64_STANDARD.encode(&events_bcs),
                        "confirmedLocalExecution": true,
                    }))
                }
            },
        )?;
    }

    // --- sui_dryRunTransactionBlock ---
    {
        let node = node.clone();
        module.register_async_method(
            "sui_dryRunTransactionBlock",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let tx_bytes_b64: String = p.next().map_err(internal_error)?;
                    let tx_bytes = BASE64_STANDARD
                        .decode(&tx_bytes_b64)
                        .map_err(|e| internal_error(format!("base64 decode: {e}")))?;
                    let tx_data: sui_types::transaction::TransactionData =
                        bcs::from_bytes(&tx_bytes)
                            .map_err(|e| internal_error(format!("bcs decode: {e}")))?;
                    let n = node.lock().await;
                    let (effects, events) = n
                        .dry_run_transaction(tx_data)
                        .map_err(internal_error)?;
                    let digest = effects.transaction_digest().to_string();
                    let effects_json = build_effects_json(&effects, &n.store);
                    let events_json = build_events_json(&events, effects.transaction_digest());
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "digest": digest,
                        "effects": effects_json,
                        "events": events_json,
                        "dryRun": true,
                    }))
                }
            },
        )?;
    }

    // --- sui_getTransactionBlock ---
    {
        let node = node.clone();
        module.register_async_method("sui_getTransactionBlock", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let digest_str: String = p.next().map_err(internal_error)?;
                let digest: TransactionDigest = digest_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid digest: {e}")))?;
                let n = node.lock().await;
                let local = n.store.local.read().unwrap();
                let effects = local.effects.get(&digest).cloned();
                let events = local.events.get(&digest).cloned();
                drop(local);
                match effects {
                    None => Ok(serde_json::json!({ "error": { "code": "transactionNotFound" } })),
                    Some(eff) => {
                        let events_json = events
                            .as_ref()
                            .map(|ev| build_events_json(ev, &digest))
                            .unwrap_or(Value::Array(vec![]));
                        Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                            "digest": digest.to_string(),
                            "effects": build_effects_json(&eff, &n.store),
                            "events": events_json,
                        }))
                    }
                }
            }
        })?;
    }

    // --- suix_queryEvents ---
    {
        let node = node.clone();
        module.register_async_method("suix_queryEvents", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                // Accepts either {"Transaction": "<digest>"} or {"MoveEventType": "<type>"}
                let filter: Value = p.next().map_err(internal_error)?;

                let n = node.lock().await;
                let local = n.store.local.read().unwrap();

                if let Some(digest_str) = filter.get("Transaction").and_then(|v| v.as_str()) {
                    let digest: TransactionDigest = digest_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid digest: {e}")))?;
                    let events_json = local
                        .events
                        .get(&digest)
                        .map(|ev| build_events_json(ev, &digest))
                        .unwrap_or(Value::Array(vec![]));
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "data": events_json,
                        "nextCursor": Value::Null,
                        "hasNextPage": false,
                    }))
                } else if let Some(type_str) = filter.get("MoveEventType").and_then(|v| v.as_str()) {
                    let target_type: StructTag = type_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid event type: {e}")))?;
                    let mut matched: Vec<Value> = Vec::new();
                    for (digest, tx_events) in &local.events {
                        for (i, event) in tx_events.data.iter().enumerate() {
                            if event.type_ == target_type {
                                matched.push(serde_json::json!({
                                    "id": {
                                        "txDigest": digest.to_string(),
                                        "eventSeq": i.to_string(),
                                    },
                                    "packageId": event.package_id.to_string(),
                                    "transactionModule": event.transaction_module.to_string(),
                                    "sender": event.sender.to_string(),
                                    "type": event.type_.to_string(),
                                    "bcsEncoded": BASE64_STANDARD.encode(&event.contents),
                                }));
                            }
                        }
                    }
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "data": matched,
                        "nextCursor": Value::Null,
                        "hasNextPage": false,
                    }))
                } else {
                    Err(internal_error(
                        "filter must be {\"Transaction\": \"<digest>\"} or {\"MoveEventType\": \"<type>\"}",
                    ))
                }
            }
        })?;
    }

    // --- suix_getBalance ---
    {
        let node = node.clone();
        module.register_async_method("suix_getBalance", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let coin_type: Option<String> = p.next().ok();
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let n = node.lock().await;
                let (total, count) = n.get_balance(address, coin_type.as_deref());
                let coin_type_str = coin_type.unwrap_or_else(|| "all".to_string());
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "coinType": coin_type_str,
                    "coinObjectCount": count,
                    "totalBalance": total.to_string(),
                }))
            }
        })?;
    }

    // --- suix_getAllBalances ---
    {
        let node = node.clone();
        module.register_async_method("suix_getAllBalances", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let n = node.lock().await;
                let balances = n.get_all_balances(address);
                let result: Vec<Value> = balances
                    .into_iter()
                    .map(|(coin_type, total, count)| {
                        serde_json::json!({
                            "coinType": coin_type,
                            "coinObjectCount": count,
                            "totalBalance": total.to_string(),
                        })
                    })
                    .collect();
                Ok::<Value, ErrorObjectOwned>(Value::Array(result))
            }
        })?;
    }

    // --- suix_getCoins ---
    {
        let node = node.clone();
        module.register_async_method("suix_getCoins", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let coin_type_filter: Option<String> = p.next().ok();
                // cursor and limit ignored — local store is small
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;

                let type_filter_tag: Option<StructTag> =
                    coin_type_filter.as_deref().and_then(|ct| {
                        let full = format!("0x2::coin::Coin<{ct}>");
                        full.parse().ok()
                    });

                let n = node.lock().await;
                let coins: Vec<Value> = n
                    .store
                    .owned_objects(address)
                    .filter_map(|obj| {
                        let Data::Move(ref move_obj) = obj.data else {
                            return None;
                        };
                        if !move_obj.is_coin() {
                            return None;
                        }
                        if let Some(ref filter) = type_filter_tag
                            && !obj.data.type_().is_some_and(|t| t.is(filter))
                        {
                            return None;
                        }
                        Some(serde_json::json!({
                            "coinType": move_obj.type_().to_string(),
                            "coinObjectId": obj.id().to_string(),
                            "version": obj.version().value().to_string(),
                            "digest": obj.digest().to_string(),
                            "balance": move_obj.get_coin_value_unsafe().to_string(),
                            "previousTransaction": obj.previous_transaction.to_string(),
                        }))
                    })
                    .collect();
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "data": coins,
                    "nextCursor": Value::Null,
                    "hasNextPage": false,
                }))
            }
        })?;
    }

    // --- fork_fundAccount ---
    {
        let node = node.clone();
        module.register_async_method("fork_fundAccount", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let amount: u64 = p.next().map_err(internal_error)?;
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let mut n = node.lock().await;
                let coin_id =
                    n.fund_account(address, amount).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::String(coin_id.to_string()))
            }
        })?;
    }

    // --- fork_advanceClock ---
    {
        let node = node.clone();
        module.register_async_method("fork_advanceClock", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let duration_ms: u64 = p.next().map_err(internal_error)?;
                let mut n = node.lock().await;
                n.advance_clock(std::time::Duration::from_millis(duration_ms))
                    .map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_setClockTimestamp ---
    {
        let node = node.clone();
        module.register_async_method("fork_setClockTimestamp", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let timestamp_ms: u64 = p.next().map_err(internal_error)?;
                let mut n = node.lock().await;
                n.set_clock_timestamp(timestamp_ms).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_advanceEpoch ---
    {
        let node = node.clone();
        module.register_async_method("fork_advanceEpoch", move |_, _, _| {
            let node = node.clone();
            async move {
                let mut n = node.lock().await;
                n.advance_epoch().map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_snapshot ---
    {
        let node = node.clone();
        module.register_async_method("fork_snapshot", move |_, _, _| {
            let node = node.clone();
            async move {
                let mut n = node.lock().await;
                Ok::<Value, ErrorObjectOwned>(Value::Number(n.snapshot().into()))
            }
        })?;
    }

    // --- fork_revert ---
    {
        let node = node.clone();
        module.register_async_method("fork_revert", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let snapshot_id: u64 = p.next().map_err(internal_error)?;
                let mut n = node.lock().await;
                n.revert(snapshot_id).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_reset ---
    {
        let node = node.clone();
        module.register_async_method("fork_reset", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let checkpoint: Option<u64> = p.next().ok();
                let mut n = node.lock().await;
                n.reset(checkpoint).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_seedObject ---
    {
        let node = node.clone();
        module.register_async_method("fork_seedObject", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let object_id: sui_types::base_types::ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let mut n = node.lock().await;
                let found = n.seed_object(object_id).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(found))
            }
        })?;
    }

    // --- fork_seedOwnedObjects ---
    {
        let node = node.clone();
        module.register_async_method("fork_seedOwnedObjects", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let mut n = node.lock().await;
                n.seed_owned_objects(address).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Array(vec![]))
            }
        })?;
    }

    // --- fork_dumpState ---
    {
        let node = node.clone();
        module.register_async_method("fork_dumpState", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let path_str: String = p.next().map_err(internal_error)?;
                let path = PathBuf::from(&path_str);
                let n = node.lock().await;
                n.dump_state(&path).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::String(path_str))
            }
        })?;
    }

    // --- fork_loadState ---
    {
        let node = node.clone();
        module.register_async_method("fork_loadState", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let path_str: String = p.next().map_err(internal_error)?;
                let path = PathBuf::from(&path_str);
                let mut n = node.lock().await;
                let node_config = n.node_config.clone();
                *n = crate::ForkedNode::load_state(&path, &node_config)
                    .map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_setObjectBcs ---
    {
        let node = node.clone();
        module.register_async_method("fork_setObjectBcs", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let bcs_b64: String = p.next().map_err(internal_error)?;
                let object_id: sui_types::base_types::ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let bcs_bytes = BASE64_STANDARD
                    .decode(&bcs_b64)
                    .map_err(|e| internal_error(format!("base64 decode: {e}")))?;
                let mut n = node.lock().await;
                n.set_object_bcs(object_id, &bcs_bytes)
                    .map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_getObjectBcs ---
    // Returns the full Object BCS-encoded so the CLI client can deserialize it.
    {
        let node = node.clone();
        module.register_async_method("fork_getObjectBcs", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let object_id: sui_types::base_types::ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let n = node.lock().await;
                match n.get_object(&object_id) {
                    Some(obj) => {
                        let bcs = bcs::to_bytes(&obj)
                            .map_err(|e| internal_error(format!("bcs encode: {e}")))?;
                        Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                            "bcs": BASE64_STANDARD.encode(&bcs),
                        }))
                    }
                    None => Ok(serde_json::json!({ "error": "not found" })),
                }
            }
        })?;
    }

    // --- fork_getOwnedObjectsBcs ---
    // Returns BCS-encoded objects owned by an address, filtered by struct tag.
    // Used by ForkClient to implement DataReader::get_owned_objects.
    {
        let node = node.clone();
        module.register_async_method("fork_getOwnedObjectsBcs", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let addr_str: String = p.next().map_err(internal_error)?;
                let type_str: String = p.next().map_err(internal_error)?;
                let address: sui_types::base_types::SuiAddress = addr_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                let struct_tag: StructTag = type_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid struct tag: {e}")))?;
                let n = node.lock().await;
                let results: Vec<Value> = n
                    .store
                    .owned_objects(address)
                    .filter(|obj| {
                        obj.data
                            .type_()
                            .map(|t| t.is(&struct_tag))
                            .unwrap_or(false)
                    })
                    .filter_map(|obj| {
                        let bcs = bcs::to_bytes(&obj).ok()?;
                        Some(serde_json::json!({
                            "objectId": obj.id().to_string(),
                            "bcs": BASE64_STANDARD.encode(&bcs),
                        }))
                    })
                    .collect();
                Ok::<Value, ErrorObjectOwned>(Value::Array(results))
            }
        })?;
    }

    // --- sui_getNormalizedMoveModule ---
    {
        let node = node.clone();
        module.register_async_method(
            "sui_getNormalizedMoveModule",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let pkg_id_str: String = p.next().map_err(internal_error)?;
                    let module_name: String = p.next().map_err(internal_error)?;
                    let package_id: ObjectID = pkg_id_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid package id: {e}")))?;
                    let n = node.lock().await;
                    let obj = ObjectStore::get_object(&n.store, &package_id)
                        .ok_or_else(|| internal_error(format!("package {package_id} not found")))?;
                    let Data::Package(ref pkg) = obj.data else {
                        return Err(internal_error("object is not a package"));
                    };
                    let mut pool = RcPool::new();
                    let modules = pkg
                        .normalize(&mut pool, &BinaryConfig::standard(), false)
                        .map_err(|e| internal_error(format!("normalize failed: {e}")))?;
                    let normalized = modules
                        .get(&module_name)
                        .ok_or_else(|| internal_error(format!("module '{module_name}' not found in package")))?;
                    let sui_mod = SuiMoveNormalizedModule::from(normalized);
                    serde_json::to_value(&sui_mod)
                        .map_err(|e| internal_error(format!("serialize: {e}")))
                }
            },
        )?;
    }

    // --- sui_getNormalizedMoveFunction ---
    {
        let node = node.clone();
        module.register_async_method(
            "sui_getNormalizedMoveFunction",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let pkg_id_str: String = p.next().map_err(internal_error)?;
                    let module_name: String = p.next().map_err(internal_error)?;
                    let function_name: String = p.next().map_err(internal_error)?;
                    let package_id: ObjectID = pkg_id_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid package id: {e}")))?;
                    let n = node.lock().await;
                    let obj = ObjectStore::get_object(&n.store, &package_id)
                        .ok_or_else(|| internal_error(format!("package {package_id} not found")))?;
                    let Data::Package(ref pkg) = obj.data else {
                        return Err(internal_error("object is not a package"));
                    };
                    let mut pool = RcPool::new();
                    let modules = pkg
                        .normalize(&mut pool, &BinaryConfig::standard(), false)
                        .map_err(|e| internal_error(format!("normalize failed: {e}")))?;
                    let normalized_mod = modules
                        .get(&module_name)
                        .ok_or_else(|| internal_error(format!("module '{module_name}' not found")))?;
                    let normalized_fn = normalized_mod
                        .functions
                        .iter()
                        .find(|(k, _)| k.to_string() == function_name)
                        .map(|(_, v)| v)
                        .ok_or_else(|| internal_error(format!("function '{function_name}' not found in module '{module_name}'")))?;
                    let sui_fn = SuiMoveNormalizedFunction::from(&**normalized_fn);
                    serde_json::to_value(&sui_fn)
                        .map_err(|e| internal_error(format!("serialize: {e}")))
                }
            },
        )?;
    }

    // --- fork_setOwner ---
    {
        let node = node.clone();
        module.register_async_method("fork_setOwner", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let owner_json: Value = p.next().map_err(internal_error)?;
                let object_id: ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let new_owner: Owner = serde_json::from_value(owner_json)
                    .map_err(|e| internal_error(format!("invalid owner JSON: {e}")))?;
                let mut n = node.lock().await;
                n.set_owner(object_id, new_owner).map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_getObjectHistory ---
    {
        let node = node.clone();
        module.register_async_method("fork_getObjectHistory", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let object_id: ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let n = node.lock().await;
                let history = {
                    let local = n.store.local.read().unwrap();
                    local
                        .objects
                        .get(&object_id)
                        .map(|versions| {
                            versions
                                .values()
                                .map(build_object_response)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                Ok::<Value, ErrorObjectOwned>(Value::Array(history))
            }
        })?;
    }

    // --- fork_listTransactions ---
    {
        let node = node.clone();
        module.register_async_method("fork_listTransactions", move |_, _, _| {
            let node = node.clone();
            async move {
                let n = node.lock().await;
                let local = n.store.local.read().unwrap();
                let txs: Vec<Value> = local
                    .effects
                    .iter()
                    .map(|(digest, effects)| {
                        let status = match effects.status() {
                            ExecutionStatus::Success => "success",
                            ExecutionStatus::Failure { .. } => "failure",
                        };
                        let gas = effects.gas_cost_summary();
                        serde_json::json!({
                            "digest": digest.to_string(),
                            "status": status,
                            "gasUsed": (gas.computation_cost + gas.storage_cost).to_string(),
                        })
                    })
                    .collect();
                Ok::<Value, ErrorObjectOwned>(Value::Array(txs))
            }
        })?;
    }

    // --- suix_getDynamicFields ---
    {
        let node = node.clone();
        module.register_async_method("suix_getDynamicFields", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let parent_id: ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let n = node.lock().await;
                let children: Vec<Value> = {
                    let local = n.store.local.read().unwrap();
                    let parent_addr = sui_types::base_types::SuiAddress::from(parent_id);
                    local
                        .live_objects
                        .iter()
                        .filter_map(|(id, version)| {
                            local.objects.get(id)?.get(version).cloned()
                        })
                        .filter(|obj| obj.owner == Owner::ObjectOwner(parent_addr))
                        .map(|obj| {
                            serde_json::json!({
                                "objectId": obj.id().to_string(),
                                "version": obj.version().value().to_string(),
                                "type": obj.data.type_().map(|t| t.to_string()).unwrap_or_default(),
                                "owner": serde_json::to_value(&obj.owner).unwrap_or(Value::Null),
                            })
                        })
                        .collect()
                };
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "data": children,
                    "nextCursor": Value::Null,
                    "hasNextPage": false,
                }))
            }
        })?;
    }

    // --- fork_decodeObject ---
    {
        let node = node.clone();
        module.register_async_method("fork_decodeObject", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let id_str: String = p.next().map_err(internal_error)?;
                let object_id: ObjectID = id_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                let n = node.lock().await;
                let obj = n
                    .get_object(&object_id)
                    .ok_or_else(|| internal_error(format!("object {object_id} not found")))?;
                let Data::Move(ref move_obj) = obj.data else {
                    return Err(internal_error(
                        "object is not a Move object — cannot decode fields",
                    ));
                };
                let cache = ModuleCache::new(&n.store);
                let move_struct = move_obj
                    .to_move_struct_with_resolver(&cache)
                    .map_err(|e| internal_error(format!("decode failed: {e}")))?;
                let sui_struct = SuiMoveStruct::from(move_struct);
                let fields_json = serde_json::to_value(&sui_struct)
                    .map_err(|e| internal_error(format!("serialize: {e}")))?;
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "objectId": obj.id().to_string(),
                    "type": move_obj.type_().to_string(),
                    "version": obj.version().value().to_string(),
                    "decodedFields": fields_json,
                }))
            }
        })?;
    }

    // --- fork_replayTransaction ---
    {
        let node = node.clone();
        module.register_async_method("fork_replayTransaction", move |params, _, _| {
            let node = node.clone();
            async move {
                let mut p = params.sequence();
                let digest_str: String = p.next().map_err(internal_error)?;
                let digest: TransactionDigest = digest_str
                    .parse()
                    .map_err(|e| internal_error(format!("invalid digest: {e}")))?;
                let mut n = node.lock().await;
                let (effects, events) =
                    n.replay_transaction(digest).map_err(internal_error)?;
                let effects_json = build_effects_json(&effects, &n.store);
                let events_json = build_events_json(&events, effects.transaction_digest());
                Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                    "digest": effects.transaction_digest().to_string(),
                    "effects": effects_json,
                    "events": events_json,
                    "replayed": true,
                }))
            }
        })?;
    }

    // --- fork_seedBridgeObjects ---
    {
        let node = node.clone();
        module.register_async_method("fork_seedBridgeObjects", move |_, _, _| {
            let node = node.clone();
            async move {
                let mut n = node.lock().await;
                n.seed_bridge_objects().map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_setupBridgeTestCommittee ---
    {
        let node = node.clone();
        module.register_async_method("fork_setupBridgeTestCommittee", move |_, _, _| {
            let node = node.clone();
            async move {
                let mut n = node.lock().await;
                n.setup_bridge_test_committee().map_err(internal_error)?;
                Ok::<Value, ErrorObjectOwned>(Value::Bool(true))
            }
        })?;
    }

    // --- fork_simulateEthToSuiBridge ---
    {
        let node = node.clone();
        module.register_async_method(
            "fork_simulateEthToSuiBridge",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let recipient_str: String = p.next().map_err(internal_error)?;
                    let token_id: u8 = p.next().map_err(internal_error)?;
                    let amount: u64 = p.next().map_err(internal_error)?;
                    let nonce: u64 = p.next().map_err(internal_error)?;
                    let eth_chain_id: u8 = p.next().unwrap_or(11); // default EthSepolia
                    let recipient: sui_types::base_types::SuiAddress = recipient_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid address: {e}")))?;
                    let mut n = node.lock().await;
                    let (effects, events) = n
                        .simulate_eth_to_sui_bridge(
                            recipient, token_id, amount, nonce, eth_chain_id,
                        )
                        .map_err(internal_error)?;
                    let effects_json = build_effects_json(&effects, &n.store);
                    let events_json = build_events_json(&events, effects.transaction_digest());
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "digest": effects.transaction_digest().to_string(),
                        "effects": effects_json,
                        "events": events_json,
                    }))
                }
            },
        )?;
    }

    // --- fork_simulateSuiToEthBridge ---
    {
        let node = node.clone();
        module.register_async_method(
            "fork_simulateSuiToEthBridge",
            move |params, _, _| {
                let node = node.clone();
                async move {
                    let mut p = params.sequence();
                    let sender_str: String = p.next().map_err(internal_error)?;
                    let token_id_str: String = p.next().map_err(internal_error)?;
                    let eth_chain_id: u8 = p.next().unwrap_or(10); // default EthMainnet
                    let eth_recipient_hex: String = p.next().map_err(internal_error)?;
                    let gas_budget: u64 = p.next().unwrap_or(100_000_000);
                    let sender: sui_types::base_types::SuiAddress = sender_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid sender: {e}")))?;
                    let token_object_id: ObjectID = token_id_str
                        .parse()
                        .map_err(|e| internal_error(format!("invalid object id: {e}")))?;
                    // Decode hex eth_recipient into bytes (strip leading 0x if present)
                    let hex_clean = eth_recipient_hex.strip_prefix("0x").unwrap_or(&eth_recipient_hex);
                    let eth_recipient = hex::decode(hex_clean)
                        .map_err(|e| internal_error(format!("invalid eth address hex: {e}")))?;
                    let mut n = node.lock().await;
                    let (effects, events) = n
                        .simulate_sui_to_eth_bridge(
                            sender,
                            token_object_id,
                            eth_chain_id,
                            eth_recipient,
                            gas_budget,
                        )
                        .map_err(internal_error)?;
                    let effects_json = build_effects_json(&effects, &n.store);
                    let events_json = build_events_json(&events, effects.transaction_digest());
                    Ok::<Value, ErrorObjectOwned>(serde_json::json!({
                        "digest": effects.transaction_digest().to_string(),
                        "effects": effects_json,
                        "events": events_json,
                    }))
                }
            },
        )?;
    }

    let handle = server.start(module);
    tracing::info!("sui fork RPC server listening on http://{addr}");
    handle.stopped().await;
    Ok(())
}
