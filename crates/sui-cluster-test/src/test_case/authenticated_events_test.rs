// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use async_trait::async_trait;
use fastcrypto::hash::{Blake2b256, HashFunction};
use move_core_types::account_address::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use sui_indexer::types::owner_to_owner_info;
use sui_json_rpc_types::{SuiMoveValue, SuiTransactionBlockEffectsAPI};
use sui_package_resolver::{error::Error as PackageResolverError, Package, PackageStore, Resolver};
use sui_rpc_api::Client as RpcClient;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::event::Event;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::object::bounded_visitor::BoundedVisitor;
use sui_types::object::Owner;
use sui_types::transaction::CallArg;
use sui_types::{TypeTag, SUI_ACCUMULATOR_ROOT_ADDRESS};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::{TestCaseImpl, TestContext};

/// Represents the on-chain state of an authenticated event stream's head.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StreamHead {
    /// The Merkle root of all events in the stream for a given checkpoint.
    pub root: Vec<u8>,
    /// The digest of the previous StreamHead object, forming a hash chain.
    pub prev: Vec<u8>,
}

pub struct AuthenticatedEventsTest;

#[async_trait]
impl TestCaseImpl for AuthenticatedEventsTest {
    fn name(&self) -> &'static str {
        "AuthenticatedEventsTest"
    }

    fn description(&self) -> &'static str {
        "Verifies authenticated events by checking on-chain stream heads against emitted events."
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<()> {
        let harness = TestHarness::new(ctx).await?;

        let package_id = harness.publish_package_and_emit_event().await?;
        info!("Published test package with ID: {}", package_id);

        let mut checkpoint_num = 0;
        let mut num_verified_updates = 0;
        let mut latest_verified_stream_head: Option<StreamHead> = None;

        loop {
            let checkpoint_data = harness
                .get_checkpoint_data_with_retry(checkpoint_num)
                .await?;

            if let Some(new_head) = harness
                .process_checkpoint(&checkpoint_data, package_id)
                .await?
            {
                // Verify the new head against the events in this checkpoint.
                let stream_events =
                    read_default_stream_events_from_checkpoint(&checkpoint_data, package_id);
                verify_events_against_stream_head(
                    &stream_events,
                    latest_verified_stream_head.as_ref(),
                    &new_head,
                );
                info!(
                    "Verified events against stream head in checkpoint {}",
                    checkpoint_num
                );
                latest_verified_stream_head = Some(new_head);
                num_verified_updates += 1;

                // For testing purposes, emit another event to create a new update to find.
                harness.emit_event_to_default_stream(package_id).await?;
            }

            info!("Processed checkpoint {}", checkpoint_num);
            checkpoint_num += 1;
            if num_verified_updates > 10 {
                info!("Test finished successfully after verifying stream updates.");
                return Ok(());
            }
        }
    }
}

/// Encapsulates the state and logic for running the authenticated events test.
struct TestHarness<'a> {
    wallet: &'a sui_sdk::wallet_context::WalletContext,
    rpc: RpcClient,
    resolver: Resolver<FullNodePackageStore>,
}

impl<'a> TestHarness<'a> {
    /// Creates a new TestHarness instance.
    pub async fn new(ctx: &'a TestContext) -> Result<Self> {
        let rpc_url = ctx.get_fullnode_rpc_url();
        info!("Using fullnode RPC: {}", rpc_url);
        let rpc = RpcClient::new(rpc_url.to_string())?;
        let resolver = Resolver::new(FullNodePackageStore::new(rpc.clone()));
        Ok(Self {
            wallet: ctx.get_wallet(),
            rpc,
            resolver,
        })
    }

    async fn get_checkpoint_data_with_retry(&self, checkpoint_num: u64) -> Result<CheckpointData> {
        loop {
            if let Ok(checkpoint_data) = self.rpc.get_full_checkpoint(checkpoint_num).await {
                return Ok(checkpoint_data);
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }

    /// Processes a single checkpoint to find updates to our specific event stream.
    async fn process_checkpoint(
        &self,
        checkpoint_data: &CheckpointData,
        package_id: ObjectID,
    ) -> Result<Option<StreamHead>> {
        for transaction in &checkpoint_data.transactions {
            for object in &transaction.output_objects {
                if let Some(head) = self
                    .find_stream_head_update_in_object(object, package_id)
                    .await?
                {
                    return Ok(Some(head));
                }
            }
        }
        Ok(None)
    }

    /// Parses a single object to see if it's the stream head we are looking for.
    async fn find_stream_head_update_in_object(
        &self,
        object: &sui_types::object::Object,
        package_id: ObjectID,
    ) -> Result<Option<StreamHead>> {
        // Filter 1: Must be a dynamic field owned by the accumulator root.
        let Some(parent_id) = owner_to_owner_info(&object.owner).1 else {
            return Ok(None);
        };
        if parent_id != SUI_ACCUMULATOR_ROOT_ADDRESS.into() {
            return Ok(None);
        }

        let move_obj_opt = object.data.try_as_move();
        let Some(move_object) = move_obj_opt else {
            return Ok(None);
        };
        if !move_object.type_().is_dynamic_field() {
            return Ok(None);
        }

        let layout = self
            .resolver
            .type_layout(move_object.type_().clone().into())
            .await?;
        let field = DFV::FieldVisitor::deserialize(move_object.contents(), &layout)?;
        let name_type: TypeTag = field.name_layout.into();

        if name_type.to_canonical_string(true) != "0x0000000000000000000000000000000000000000000000000000000000000002::accumulator::Key" {
            return Ok(None);
        }

        // Deserialize the key's value to check the stream ID.
        let name_value = BoundedVisitor::deserialize_value(field.name_bytes, field.name_layout)
            .context("Failed to deserialize dynamic field name")?;

        if let SuiMoveValue::Struct(key_struct) = SuiMoveValue::from(name_value) {
            if let Some(SuiMoveValue::Address(stream_id_from_key)) =
                key_struct.field_value("address")
            {
                // Filter 3: The stream ID in the key must match our package ID.
                if stream_id_from_key == package_id.into() {
                    // If all filters pass, deserialize the value (the StreamHead).
                    let value =
                        BoundedVisitor::deserialize_value(field.value_bytes, field.value_layout)
                            .context("Failed to deserialize dynamic field value (StreamHead)")?;
                    let SuiMoveValue::Struct(value_struct) = SuiMoveValue::from(value) else {
                        return Ok(None);
                    };

                    let root_bytes: Vec<u8> = if let Some(SuiMoveValue::Vector(root_vec)) =
                        value_struct.field_value("root")
                    {
                        root_vec
                            .iter()
                            .map(|val| {
                                if let SuiMoveValue::Number(n) = val {
                                    *n as u8
                                } else {
                                    0
                                }
                            })
                            .collect()
                    } else {
                        vec![]
                    };

                    let prev_bytes: Vec<u8> = if let Some(SuiMoveValue::Vector(prev_vec)) =
                        value_struct.field_value("prev")
                    {
                        prev_vec
                            .iter()
                            .map(|val| {
                                if let SuiMoveValue::Number(n) = val {
                                    *n as u8
                                } else {
                                    0
                                }
                            })
                            .collect()
                    } else {
                        vec![]
                    };

                    let stream_head = StreamHead {
                        root: root_bytes,
                        prev: prev_bytes,
                    };
                    return Ok(Some(stream_head));
                }
            }
        }

        Ok(None)
    }

    pub async fn publish_package_and_emit_event(&self) -> Result<ObjectID> {
        use sui_test_transaction_builder::TestTransactionBuilder;
        let mut package_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        package_path.push("contracts/authenticated_events");
        let (sender, gas_objects) = self.wallet.get_one_account().await?;
        let gas_object = gas_objects[0];
        let rgp = self.wallet.get_reference_gas_price().await?;

        let transaction = TestTransactionBuilder::new(sender, gas_object, rgp)
            .publish(package_path)
            .build();
        let signed_tx = self.wallet.sign_transaction(&transaction);
        let response = self
            .wallet
            .execute_transaction_must_succeed(signed_tx)
            .await;
        let Some(effects) = response.effects else {
            panic!("No effects found");
        };
        let gas_object = gas_objects[1];
        let package_id = effects
            .created()
            .iter()
            .find(|obj| matches!(obj.owner, Owner::Immutable))
            .unwrap()
            .reference
            .to_object_ref()
            .0;
        let signed_tx = self.wallet.sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .move_call(
                    package_id,
                    "authenticated_events",
                    "emit_to_default_stream",
                    vec![CallArg::Pure(bcs::to_bytes(&(1_u64))?)],
                )
                .build(),
        );
        let response = self
            .wallet
            .execute_transaction_must_succeed(signed_tx)
            .await;
        let Some(_effects) = response.effects else {
            panic!("No effects found");
        };
        Ok(package_id)
    }

    /// Submits a transaction to emit an event to the package's default stream.
    pub async fn emit_event_to_default_stream(&self, package_id: ObjectID) -> Result<()> {
        let (sender, gas_objects) = self.wallet.get_one_account().await?;
        let gas_object = gas_objects[1];
        let rgp = self.wallet.get_reference_gas_price().await?;
        let signed_tx = self.wallet.sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, rgp)
                .move_call(
                    package_id,
                    "authenticated_events",
                    "emit_to_default_stream",
                    vec![CallArg::Pure(bcs::to_bytes(&(1_u64)).unwrap())],
                )
                .build(),
        );
        let response = self
            .wallet
            .execute_transaction_must_succeed(signed_tx)
            .await;
        let Some(_effects) = response.effects else {
            panic!("No effects found");
        };
        Ok(())
    }
}

/// Extracts all events from a checkpoint that belong to the default stream of a given package.
pub fn read_default_stream_events_from_checkpoint(
    checkpoint: &CheckpointData,
    package_id: ObjectID,
) -> Vec<Event> {
    checkpoint
        .transactions
        .iter()
        .flat_map(|tx| tx.events.clone().unwrap_or_default().data)
        .filter(|event| event.package_id == package_id)
        .collect()
}

pub fn hash_helper(data: &[u8]) -> Vec<u8> {
    let mut h = Blake2b256::new();
    h.update(data);
    h.finalize().to_vec()
}

pub fn in_stream_commitment(events: &[Event]) -> Vec<u8> {
    let mut event_hashes = Vec::new();
    for event in events.iter() {
        event_hashes.push(hash_helper(&bcs::to_bytes(&event).unwrap()));
    }
    hash_helper(&bcs::to_bytes(&event_hashes).unwrap())
}

pub fn verify_events_against_stream_head(
    events: &[Event],
    old_stream_head: Option<&StreamHead>,
    new_stream_head: &StreamHead,
) {
    let commitment = in_stream_commitment(events);
    assert!(commitment == new_stream_head.root);

    if old_stream_head.is_some() {
        let prev_head = old_stream_head.as_ref().unwrap();
        let digest = hash_helper(&bcs::to_bytes(prev_head).unwrap());
        assert_eq!(digest, new_stream_head.prev);
    } else {
        assert_eq!(new_stream_head.prev, [0; 32]);
    }
}

pub struct FullNodePackageStore {
    rpc_client: RpcClient,
    cache: Mutex<HashMap<AccountAddress, Arc<Package>>>,
}

impl FullNodePackageStore {
    pub fn new(rpc_client: RpcClient) -> Self {
        Self {
            rpc_client,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PackageStore for FullNodePackageStore {
    async fn fetch(&self, id: AccountAddress) -> sui_package_resolver::Result<Arc<Package>> {
        // Check if we have it in the cache
        let res: anyhow::Result<Arc<Package>> = async move {
            if let Some(package) = self.cache.lock().await.get(&id) {
                return Ok(package.clone());
            }

            let object = self.rpc_client.get_object(id.into()).await?;
            let package = Arc::new(Package::read_from_object(&object)?);

            self.cache.lock().await.insert(id, package.clone());

            Ok(package)
        }
        .await;
        res.map_err(|e| {
            error!("Fetch Package: {} error: {:?}", id, e);
            PackageResolverError::PackageNotFound(id)
        })
    }
}
