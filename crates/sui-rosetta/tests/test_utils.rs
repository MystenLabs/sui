// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Test utilities shared across sui-rosetta tests

#![allow(dead_code)]

use anyhow::Result;
use prost_types::FieldMask;
use rand::rngs::OsRng;
use rand::seq::IteratorRandom;
use std::time::Duration;
use sui_rosetta::errors::Error;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    Bcs, ChangedObject, ExecuteTransactionRequest, ExecutedTransaction, GetObjectRequest,
    GetTransactionRequest, ListOwnedObjectsRequest, Transaction as GrpcTransaction, UserSignature,
};
use sui_types::{
    base_types::{FullObjectRef, ObjectID, ObjectRef, SuiAddress},
    coin::Coin,
    object::Object,
    transaction::Transaction,
};

/// Helper function to get all coins for an address using gRPC list_owned_objects
/// This replaces get_all_coins JSON-RPC calls with native gRPC implementation
pub async fn get_all_coins(client: &mut GrpcClient, address: SuiAddress) -> Result<Vec<Object>> {
    get_coins_by_type(client, address, None).await
}

/// Helper function to get coins of a specific type for an address using gRPC list_owned_objects
pub async fn get_coins_by_type(
    client: &mut GrpcClient,
    address: SuiAddress,
    coin_type: Option<&str>,
) -> Result<Vec<Object>> {
    use futures::TryStreamExt;

    let object_type = match coin_type {
        Some(coin_type) => format!("0x2::coin::Coin<{}>", coin_type),
        None => "0x2::coin::Coin".to_string(),
    };

    let request = ListOwnedObjectsRequest::default()
        .with_owner(address.to_string())
        .with_object_type(object_type)
        .with_read_mask(FieldMask {
            paths: vec![
                "bcs".to_string(), // BCS serialized object data
            ],
        });

    let objects = client
        .list_owned_objects(request)
        .map_err(Error::from)
        .and_then(|grpc_object| async move {
            if let Some(bcs) = &grpc_object.bcs {
                if let Ok(object) = bcs.deserialize::<Object>() {
                    Ok(object)
                } else {
                    Err(Error::DataError("Failed to deserialize object".to_string()))
                }
            } else {
                Err(Error::DataError("Missing BCS data in object".to_string()))
            }
        })
        .try_collect()
        .await?;

    Ok(objects)
}

pub async fn get_random_sui(
    client: &mut GrpcClient,
    sender: SuiAddress,
    except: Vec<ObjectID>,
) -> ObjectRef {
    let coins = get_coins_by_type(
        client,
        sender,
        Some("0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"),
    )
    .await
    .unwrap();

    let coin = coins
        .iter()
        .filter(|coin_obj| !except.contains(&coin_obj.id()))
        .choose(&mut OsRng)
        .unwrap();

    coin.compute_object_reference()
}

/// Extract coin value from Object using the same helper as sui-core
pub fn get_coin_value(object: &Object) -> u64 {
    Coin::extract_balance_if_coin(object)
        .expect("Object should be a coin")
        .expect("Coin should have valid balance data")
        .1
}

/// Extract object reference from changed_objects in transaction response
pub fn extract_object_ref_from_changed_objects(
    changed_objects: &[ChangedObject],
    object_id: ObjectID,
) -> Result<FullObjectRef> {
    let changed_object = changed_objects
        .iter()
        .find(|obj| {
            obj.object_id_opt()
                .and_then(|id| ObjectID::from_hex_literal(id).ok())
                .map(|id| id == object_id)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            anyhow::anyhow!("Object with ID {} not found in changed_objects", object_id)
        })?;

    let output_version = changed_object
        .output_version
        .ok_or_else(|| anyhow::anyhow!("Missing output_version for object {}", object_id))?;

    let output_digest = changed_object
        .output_digest_opt()
        .ok_or_else(|| anyhow::anyhow!("Missing output_digest for object {}", object_id))?
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid digest for object {}: {}", object_id, e))?;

    let object_ref: ObjectRef = (object_id, output_version.into(), output_digest);
    Ok(FullObjectRef::from_fastpath_ref(object_ref))
}

pub async fn get_object_ref(client: &mut GrpcClient, object_id: ObjectID) -> Result<FullObjectRef> {
    let request = GetObjectRequest::new(&object_id.into()).with_read_mask(FieldMask {
        paths: vec![
            "object_id".to_string(),
            "version".to_string(),
            "digest".to_string(),
        ],
    });

    let response = client
        .ledger_client()
        .get_object(request)
        .await?
        .into_inner();

    if let Some(grpc_object) = response.object
        && let (Some(object_id_str), Some(version), Some(digest_str)) = (
            &grpc_object.object_id,
            grpc_object.version,
            &grpc_object.digest,
        )
    {
        let object_id = ObjectID::from_hex_literal(object_id_str)
            .map_err(|e| anyhow::anyhow!("Invalid object_id: {}", e))?;
        let digest = digest_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid digest: {}", e))?;

        let object_ref: ObjectRef = (object_id, version.into(), digest);
        return Ok(FullObjectRef::from_fastpath_ref(object_ref));
    }

    Err(anyhow::anyhow!(
        "Failed to get fresh object reference for ObjectID: {}",
        object_id
    ))
}

/// Execute a transaction using gRPC with WaitForLocalExecution behavior
/// This replaces execute_transaction_block calls with native gRPC implementation
pub async fn execute_transaction(
    client: &mut GrpcClient,
    signed_transaction: &Transaction,
) -> Result<ExecutedTransaction> {
    // Execute the transaction
    let mut proto_transaction = GrpcTransaction::default();
    proto_transaction.bcs = Some(Bcs::serialize(signed_transaction.transaction_data()).unwrap());

    let signatures = signed_transaction
        .tx_signatures()
        .iter()
        .map(|s| {
            let mut sig = UserSignature::default();
            let mut bcs = Bcs::default();
            bcs.name = None;
            bcs.value = Some(s.as_ref().to_owned().into());
            sig.bcs = Some(bcs);
            sig
        })
        .collect();

    let exec_request = ExecuteTransactionRequest::default()
        .with_transaction(proto_transaction)
        .with_signatures(signatures)
        .with_read_mask(FieldMask::from_paths(["*"]));

    let response = client
        .execute_transaction_and_wait_for_checkpoint(exec_request, Duration::from_secs(20))
        .await
        .inspect_err(|e| {
            if let sui_rpc::client::ExecuteAndWaitError::CheckpointTimeout(response) = e {
                eprintln!(
                    "txn status: {:?}",
                    response.get_ref().transaction().effects().status()
                );
            }
        })
        .ok() // errors can be huge, avoid printing them if unwrap fails
        .unwrap()
        .into_inner()
        .transaction()
        .to_owned();

    Ok(response)
}

/// Wait for a transaction to be available in the ledger AND indexed (equivalent to WaitForLocalExecution)
pub async fn wait_for_transaction(client: &mut GrpcClient, digest: &str) -> Result<()> {
    const WAIT_FOR_LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
    const WAIT_FOR_LOCAL_EXECUTION_DELAY: Duration = Duration::from_millis(200);
    const WAIT_FOR_LOCAL_EXECUTION_INTERVAL: Duration = Duration::from_millis(500);

    let mut client = client.ledger_client();

    tokio::time::timeout(WAIT_FOR_LOCAL_EXECUTION_TIMEOUT, async {
        // Apply a short delay to give the full node a chance to catch up.
        tokio::time::sleep(WAIT_FOR_LOCAL_EXECUTION_DELAY).await;

        let mut interval = tokio::time::interval(WAIT_FOR_LOCAL_EXECUTION_INTERVAL);
        loop {
            interval.tick().await;

            let request = GetTransactionRequest::default()
                .with_digest(digest.to_owned())
                .with_read_mask(prost_types::FieldMask::from_paths(["digest", "checkpoint"]));

            if let Ok(response) = client.get_transaction(request).await {
                let tx = response.into_inner().transaction;
                if let Some(executed_tx) = tx {
                    // Check that transaction is indexed (checkpoint field is populated)
                    if executed_tx.checkpoint.is_some() {
                        break;
                    }
                }
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Timeout waiting for transaction indexing: {}", digest))?;

    Ok(())
}

/// Find the published package ID from gRPC changed objects
pub fn find_published_package(changed_objects: &[ChangedObject]) -> Result<ObjectID> {
    for obj in changed_objects {
        if let Some(object_type) = &obj.object_type {
            // Packages have type "0x1::package::UpgradeCap" or similar
            if object_type.contains("UpgradeCap") || object_type.contains("Package") {
                // The package ID is in the object_type string for UpgradeCap
                // Format: "0x1::package::UpgradeCap<{package_id}>"
                if let Some(start) = object_type.find('<')
                    && let Some(end) = object_type.find('>')
                {
                    let package_str = &object_type[start + 1..end];
                    if let Ok(package_id) = ObjectID::from_hex_literal(package_str) {
                        return Ok(package_id);
                    }
                }
            }
        }
    }

    // If no UpgradeCap found, look for any package object
    for obj in changed_objects {
        if let Some(object_type) = &obj.object_type
            && (object_type == "package" || object_type.is_empty())
            && let Some(object_id_str) = &obj.object_id
            && let Ok(object_id) = ObjectID::from_hex_literal(object_id_str)
        {
            // Check if this looks like a package ID (usually starts with 0x...)
            return Ok(object_id);
        }
    }

    Err(anyhow::anyhow!(
        "No published package found in changed objects"
    ))
}

/// Find a module object from gRPC changed objects based on a type predicate
pub fn find_module_object(
    changed_objects: &[ChangedObject],
    type_pred: impl Fn(&str) -> bool,
) -> Result<(SuiAddress, ObjectRef)> {
    let mut results = Vec::new();

    for obj in changed_objects {
        if let Some(object_type) = &obj.object_type
            && type_pred(object_type)
        {
            // Extract object information
            let object_id = obj
                .object_id_opt()
                .and_then(|id| ObjectID::from_hex_literal(id).ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid object ID"))?;

            let version = obj
                .output_version
                .ok_or_else(|| anyhow::anyhow!("Missing output version"))?;

            let digest = obj
                .output_digest_opt()
                .and_then(|d| d.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid digest"))?;

            // For created objects, owner information is typically in the object_type
            // or we need to fetch it separately. For now, use a placeholder.
            // This matches the original behavior where owner info comes from ObjectChange::Created
            let owner = SuiAddress::ZERO; // Will be updated by caller if needed

            results.push((owner, (object_id, version.into(), digest)));
        }
    }

    // Check that there is only one object found, and hence no ambiguity.
    if results.len() != 1 {
        return Err(anyhow::anyhow!(
            "Expected exactly one object, found {}",
            results.len()
        ));
    }

    Ok(results.pop().unwrap())
}
