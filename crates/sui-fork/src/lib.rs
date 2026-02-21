// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use sui_bridge::crypto::BridgeAuthorityKeyPair;
use simulacrum::epoch_state::EpochState;
use simulacrum::SimulatorStore;
use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_data_store::node::Node;
use sui_data_store::{ObjectKey, ObjectStore as RemoteObjectStore, VersionQuery};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::{ConsensusCommitDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::inner_temporary_store::InnerTemporaryStore;
use sui_types::messages_consensus::ConsensusDeterminedVersionAssignments;
use sui_types::object::{Data, Object, Owner};
use sui_types::storage::ObjectStore;
use sui_types::transaction::{
    EndOfEpochTransactionKind, Transaction, TransactionData, VerifiedTransaction,
};
use tokio::sync::Mutex;
use tracing::info;

pub mod bootstrap;
pub mod client;
pub mod commands;
mod persistence;
mod rpc;
pub mod snapshot;
pub mod store;

use self::snapshot::ForkedStoreSnapshot;
use self::store::ForkedStore;

/// Configuration for creating a forked network.
pub struct ForkConfig {
    pub node: Node,
    pub checkpoint: Option<u64>,
    pub rpc_port: u16,
    /// Optional path to a previously dumped state file. When set, the fork is
    /// restored from this file instead of bootstrapping from the remote network.
    pub state_file: Option<std::path::PathBuf>,
}

/// A local Sui network forked from a remote network at a specific checkpoint.
pub struct ForkedNode {
    pub(crate) epoch_state: EpochState,
    pub(crate) store: ForkedStore,
    pub(crate) deny_config: TransactionDenyConfig,
    pub(crate) verifier_signing_config: VerifierSigningConfig,
    pub fork_checkpoint: u64,
    pub chain_id: String,
    /// Saved node config, needed to reconstruct the DataStore when loading state via RPC.
    pub(crate) node_config: Node,
    snapshots: HashMap<u64, ForkedStoreSnapshot>,
    next_snapshot_id: u64,
    /// Keypair used for bridge committee signing in simulations.
    /// Set by `setup_bridge_test_committee` and consumed by `simulate_eth_to_sui_bridge`.
    pub(crate) bridge_test_keypair: Option<BridgeAuthorityKeyPair>,
}

impl ForkedNode {
    /// Execute a transaction, bypassing signature verification (impersonation).
    /// The transaction sender does not need to hold a private key.
    pub fn execute_transaction(
        &mut self,
        tx_data: TransactionData,
    ) -> Result<(TransactionEffects, TransactionEvents)> {
        let transaction = Transaction::from_generic_sig_data(tx_data, vec![]);
        let verified_tx = VerifiedTransaction::new_unchecked(transaction);

        let (inner_temporary_store, _, effects, execution_result) =
            self.epoch_state.execute_transaction(
                &self.store,
                &self.deny_config,
                &self.verifier_signing_config,
                &verified_tx,
            )?;

        if let Err(e) = execution_result {
            return Err(anyhow!("Transaction execution failed: {e}"));
        }

        let InnerTemporaryStore { written, events, .. } = inner_temporary_store;

        self.store.insert_executed_transaction(
            verified_tx,
            effects.clone(),
            events.clone(),
            written,
        );

        Ok((effects, events))
    }

    /// Execute a transaction without committing state changes (dry run).
    /// Effects are computed and returned but no objects are written to the store.
    /// Remote objects fetched during execution are cached locally as a side-effect.
    pub fn dry_run_transaction(
        &self,
        tx_data: TransactionData,
    ) -> Result<(TransactionEffects, TransactionEvents)> {
        let transaction = Transaction::from_generic_sig_data(tx_data, vec![]);
        let verified_tx = VerifiedTransaction::new_unchecked(transaction);

        let (inner_temporary_store, _, effects, execution_result) =
            self.epoch_state.execute_transaction(
                &self.store,
                &self.deny_config,
                &self.verifier_signing_config,
                &verified_tx,
            )?;

        if let Err(e) = execution_result {
            return Err(anyhow!("Dry-run execution failed: {e}"));
        }

        let InnerTemporaryStore { events, .. } = inner_temporary_store;
        // Discard written objects — this is a dry run, no state mutation.
        Ok((effects, events))
    }

    /// Advance the chain clock by `duration`.
    pub fn advance_clock(&mut self, duration: Duration) -> Result<()> {
        let epoch = self.epoch_state.epoch();
        let round = self.epoch_state.next_consensus_round();
        let timestamp_ms = simulacrum::store::SimulatorStore::get_clock(&self.store).timestamp_ms()
            + duration.as_millis() as u64;

        let tx = VerifiedTransaction::new_consensus_commit_prologue_v3(
            epoch,
            round,
            timestamp_ms,
            ConsensusCommitDigest::default(),
            ConsensusDeterminedVersionAssignments::empty_for_testing(),
        );

        let (inner_temporary_store, _, effects, execution_result) = self.epoch_state.execute_transaction(
            &self.store,
            &self.deny_config,
            &self.verifier_signing_config,
            &tx,
        )?;

        execution_result.map_err(|e| anyhow!("advance_clock failed: {e}"))?;

        let InnerTemporaryStore { written, events, .. } = inner_temporary_store;
        self.store
            .insert_executed_transaction(tx, effects, events, written);
        Ok(())
    }

    /// Set the chain clock to an absolute timestamp.
    /// The timestamp must be strictly greater than the current clock value (monotonic).
    pub fn set_clock_timestamp(&mut self, timestamp_ms: u64) -> Result<()> {
        let current_ms =
            simulacrum::store::SimulatorStore::get_clock(&self.store).timestamp_ms();
        if timestamp_ms <= current_ms {
            return Err(anyhow!(
                "New timestamp {timestamp_ms} must be greater than current {current_ms} (clock is monotonic)"
            ));
        }

        let epoch = self.epoch_state.epoch();
        let round = self.epoch_state.next_consensus_round();
        let tx = VerifiedTransaction::new_consensus_commit_prologue_v3(
            epoch,
            round,
            timestamp_ms,
            ConsensusCommitDigest::default(),
            ConsensusDeterminedVersionAssignments::empty_for_testing(),
        );

        let (inner_temporary_store, _, effects, execution_result) =
            self.epoch_state.execute_transaction(
                &self.store,
                &self.deny_config,
                &self.verifier_signing_config,
                &tx,
            )?;

        execution_result.map_err(|e| anyhow!("set_clock_timestamp failed: {e}"))?;

        let InnerTemporaryStore { written, events, .. } = inner_temporary_store;
        self.store
            .insert_executed_transaction(tx, effects, events, written);
        Ok(())
    }

    /// Advance to the next epoch by executing an end-of-epoch transaction.
    pub fn advance_epoch(&mut self) -> Result<()> {
        let next_epoch = self.epoch_state.epoch() + 1;
        let protocol_version = self.epoch_state.protocol_version();
        let timestamp_ms =
            simulacrum::store::SimulatorStore::get_clock(&self.store).timestamp_ms();

        let kinds = vec![EndOfEpochTransactionKind::new_change_epoch(
            next_epoch,
            protocol_version,
            0, // storage_charge
            0, // computation_charge
            0, // storage_rebate
            0, // non_refundable_storage_fee
            timestamp_ms,
            vec![], // no system package upgrades
        )];

        let tx = VerifiedTransaction::new_end_of_epoch_transaction(kinds);
        let (inner_temp_store, _, effects, execution_result) = self.epoch_state.execute_transaction(
            &self.store,
            &self.deny_config,
            &self.verifier_signing_config,
            &tx,
        )?;

        execution_result.map_err(|e| anyhow!("advance_epoch failed: {e}"))?;

        let InnerTemporaryStore { written, events, .. } = inner_temp_store;
        self.store
            .insert_executed_transaction(tx, effects, events, written);

        let system_state = simulacrum::store::SimulatorStore::get_system_state(&self.store);
        self.epoch_state = EpochState::new(system_state);
        Ok(())
    }

    /// Fund an address by creating a Coin<SUI> object directly in the local store.
    pub fn fund_account(&mut self, address: SuiAddress, amount: u64) -> Result<ObjectID> {
        let id = ObjectID::random();
        let obj = Object::with_id_owner_gas_for_testing(id, address, amount);
        self.store.insert_object(obj);
        Ok(id)
    }

    /// Force-fetch an object from remote and insert it into local state.
    ///
    /// Returns `true` if the object was found at the fork checkpoint and seeded,
    /// `false` if it does not exist at that checkpoint.
    pub fn seed_object(&mut self, object_id: ObjectID) -> Result<bool> {
        let key = ObjectKey {
            object_id,
            version_query: VersionQuery::AtCheckpoint(self.fork_checkpoint),
        };
        let results = self
            .store
            .remote
            .get_objects(&[key])
            .map_err(|e| anyhow!("Failed to fetch object: {e}"))?;
        if let Some(Some((obj, _version))) = results.into_iter().next() {
            self.store.insert_object(obj);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Seed all objects owned by `address` from the remote network into local state.
    ///
    /// This requires querying GraphQL for owned objects, which is not yet supported
    /// via the DataStore API. Use `seed_object` to seed individual objects by ID.
    pub fn seed_owned_objects(&mut self, _address: SuiAddress) -> Result<Vec<ObjectID>> {
        Err(anyhow!(
            "seed_owned_objects is not yet supported. \
             Use fork_seedObject to seed individual objects by ID."
        ))
    }

    /// Take a snapshot of current state; returns the snapshot ID.
    pub fn snapshot(&mut self) -> u64 {
        let id = self.next_snapshot_id;
        self.next_snapshot_id += 1;
        let state = self.store.snapshot_local();
        let next_consensus_round = self.epoch_state.peek_next_consensus_round();
        self.snapshots.insert(
            id,
            ForkedStoreSnapshot {
                state,
                next_consensus_round,
            },
        );
        id
    }

    /// Revert to a previously taken snapshot.
    pub fn revert(&mut self, snapshot_id: u64) -> Result<()> {
        let snap = self
            .snapshots
            .get(&snapshot_id)
            .ok_or_else(|| anyhow!("Snapshot {snapshot_id} not found"))?;
        let state = snap.state.clone();
        let next_consensus_round = snap.next_consensus_round;
        self.store.restore_local(state);
        let system_state = simulacrum::store::SimulatorStore::get_system_state(&self.store);
        self.epoch_state = EpochState::new(system_state);
        // Restore round counter so clock advances after revert get unique digests.
        self.epoch_state.set_next_consensus_round(next_consensus_round);
        // Snapshots taken after this one are now invalid — clear them so they
        // cannot be used, and reset the counter so subsequent snapshots get
        // contiguous IDs.
        self.snapshots.retain(|id, _| *id <= snapshot_id);
        self.next_snapshot_id = snapshot_id + 1;
        Ok(())
    }

    /// Reset the fork to a clean state, re-seeding system objects from remote.
    /// Optionally switch to a different checkpoint. All snapshots are cleared.
    pub fn reset(&mut self, checkpoint: Option<u64>) -> Result<()> {
        if let Some(cp) = checkpoint {
            self.fork_checkpoint = cp;
            self.store.fork_checkpoint = cp;
        }
        *self.store.local.write().unwrap() = self::store::LocalState::default();
        self.snapshots.clear();
        self.next_snapshot_id = 0;

        bootstrap::seed_objects(
            &self.store,
            bootstrap::SYSTEM_OBJECT_IDS,
            self.fork_checkpoint,
        )?;
        bootstrap::seed_objects(
            &self.store,
            bootstrap::SYSTEM_PACKAGE_IDS,
            self.fork_checkpoint,
        )?;

        let system_state = simulacrum::store::SimulatorStore::get_system_state(&self.store);
        self.epoch_state = EpochState::new(system_state);
        Ok(())
    }

    /// Compute the SUI (or other coin) balance for an address.
    ///
    /// `coin_type` is the inner type parameter, e.g. `"0x2::sui::SUI"`.
    /// If omitted, all coin types are summed together.
    ///
    /// **Note:** Only objects already in local state are counted. Objects still
    /// on the remote network that haven't been seeded will not appear here.
    pub fn get_balance(&self, address: SuiAddress, coin_type: Option<&str>) -> (u128, usize) {
        use move_core_types::language_storage::StructTag;

        let type_filter: Option<StructTag> = coin_type.and_then(|ct| {
            let full = format!("0x2::coin::Coin<{ct}>");
            full.parse().ok()
        });

        let mut total: u128 = 0;
        let mut count = 0;
        for obj in self.store.owned_objects(address) {
            let Data::Move(ref move_obj) = obj.data else {
                continue;
            };
            if !move_obj.is_coin() {
                continue;
            }
            if let Some(ref filter) = type_filter
                && !obj.data.type_().is_some_and(|t| t.is(filter))
            {
                continue;
            }
            total += move_obj.get_coin_value_unsafe() as u128;
            count += 1;
        }
        (total, count)
    }

    /// Return balances for all coin types owned by `address`.
    ///
    /// Returns a list of `(full_coin_type, total_balance, object_count)` entries.
    /// `full_coin_type` is the complete type string, e.g. `"0x2::coin::Coin<0x2::sui::SUI>"`.
    ///
    /// **Note:** Only objects already in local state are counted.
    pub fn get_all_balances(&self, address: SuiAddress) -> Vec<(String, u128, usize)> {
        let mut balances: HashMap<String, (u128, usize)> = HashMap::new();
        for obj in self.store.owned_objects(address) {
            let Data::Move(ref move_obj) = obj.data else {
                continue;
            };
            if !move_obj.is_coin() {
                continue;
            }
            let key = move_obj.type_().to_string();
            let entry = balances.entry(key).or_insert((0, 0));
            entry.0 += move_obj.get_coin_value_unsafe() as u128;
            entry.1 += 1;
        }
        balances.into_iter().map(|(k, (bal, cnt))| (k, bal, cnt)).collect()
    }

    /// Directly replace an object in local state by deserializing it from BCS bytes.
    /// The object's ID in the BCS data must match `object_id`.
    pub fn set_object_bcs(&mut self, object_id: ObjectID, bcs_bytes: &[u8]) -> Result<()> {
        let obj: Object = bcs::from_bytes(bcs_bytes)
            .map_err(|e| anyhow!("failed to deserialize object from BCS: {e}"))?;
        if obj.id() != object_id {
            return Err(anyhow!(
                "object ID mismatch: expected {object_id}, got {}",
                obj.id()
            ));
        }
        self.store.insert_object(obj);
        Ok(())
    }

    /// Serialize the current fork state to a file.
    /// The file can be loaded later with `load_state` or via `sui fork start --state`.
    pub fn dump_state(&self, path: &Path) -> Result<()> {
        persistence::dump(self, path)
    }

    /// Load a previously dumped fork state from a file.
    /// The remote DataStore is reconstructed from `node_config`.
    pub fn load_state(path: &Path, node_config: &Node) -> Result<ForkedNode> {
        persistence::load(path, node_config)
    }

    /// Return the reference gas price for the current epoch.
    pub fn reference_gas_price(&self) -> u64 {
        self.epoch_state.reference_gas_price()
    }

    /// Get an object from the local store (with remote fallback).
    pub fn get_object(&self, id: &ObjectID) -> Option<Object> {
        ObjectStore::get_object(&self.store, id)
    }

    /// Change the owner of any object in local state.
    /// Useful for security testing: simulate "what if this shared object was owned by an attacker?"
    pub fn set_owner(&mut self, object_id: ObjectID, new_owner: Owner) -> Result<()> {
        let mut obj = ObjectStore::get_object(&self.store, &object_id)
            .ok_or_else(|| anyhow!("object {object_id} not found"))?;
        obj.owner = new_owner;
        self.store.insert_object(obj);
        Ok(())
    }

    /// Seed the bridge object and its inner dynamic field into local fork state.
    /// Must be called before any bridge simulation method.
    pub fn seed_bridge_objects(&mut self) -> Result<()> {
        use sui_types::bridge::get_bridge_wrapper;
        use sui_types::dynamic_field::get_dynamic_field_object_from_store;

        self.seed_object(sui_types::SUI_BRIDGE_OBJECT_ID)?;
        if ObjectStore::get_object(&self.store, &sui_types::BRIDGE_PACKAGE_ID).is_none() {
            self.seed_object(sui_types::BRIDGE_PACKAGE_ID)?;
        }

        // Read the wrapper to locate the Versioned DF parent ID and current version.
        let wrapper = get_bridge_wrapper(&self.store)
            .map_err(|e| anyhow!("failed to read bridge wrapper: {e}"))?;
        let versioned_id: ObjectID = wrapper.version.id.id.bytes;
        let version: u64 = wrapper.version.version;

        // Fetch and cache the BridgeInnerV1 dynamic field (ForkedStore auto-caches on miss).
        let df_obj =
            get_dynamic_field_object_from_store(&self.store, versioned_id, &version)
                .map_err(|e| anyhow!("failed to fetch bridge inner DF: {e}"))?;
        self.store.insert_object(df_obj);
        Ok(())
    }

    /// Replace the on-chain bridge committee with a single test keypair at full voting power.
    /// After this, `simulate_eth_to_sui_bridge` can produce valid bridge certificates.
    ///
    /// Prerequisite: call `seed_bridge_objects` first.
    pub fn setup_bridge_test_committee(&mut self) -> Result<()> {
        use fastcrypto::traits::KeyPair as FcKeyPair;
        use sui_types::bridge::{BridgeInnerV1, MoveTypeCommitteeMember, get_bridge_wrapper};
        use sui_types::collection_types::{Entry, VecMap};
        use sui_types::dynamic_field::{Field, get_dynamic_field_object_from_store};

        let wrapper = get_bridge_wrapper(&self.store)
            .map_err(|e| anyhow!("bridge not seeded — call seed_bridge_objects first: {e}"))?;
        let versioned_id: ObjectID = wrapper.version.id.id.bytes;
        let version: u64 = wrapper.version.version;

        let mut df_obj =
            get_dynamic_field_object_from_store(&self.store, versioned_id, &version)
                .map_err(|e| anyhow!("failed to get bridge inner DF: {e}"))?;

        let Data::Move(ref mut move_obj) = df_obj.data else {
            return Err(anyhow!("bridge inner DF is not a Move object"));
        };

        let mut field: Field<u64, BridgeInnerV1> = bcs::from_bytes(move_obj.contents())
            .map_err(|e| anyhow!("failed to deserialize BridgeInnerV1: {e}"))?;

        let keypair = BridgeAuthorityKeyPair::generate(&mut rand::thread_rng());
        let pubkey_bytes = keypair.public().as_ref().to_vec();

        field.value.committee.members = VecMap {
            contents: vec![Entry {
                key: pubkey_bytes.clone(),
                value: MoveTypeCommitteeMember {
                    sui_address: sui_types::base_types::SuiAddress::ZERO,
                    bridge_pubkey_bytes: pubkey_bytes,
                    voting_power: 10000,
                    http_rest_url: vec![],
                    blocklisted: false,
                },
            }],
        };

        let new_contents = bcs::to_bytes(&field)
            .map_err(|e| anyhow!("failed to re-serialize BridgeInnerV1: {e}"))?;
        move_obj.set_contents_unsafe(new_contents);
        self.store.insert_object(df_obj);

        self.bridge_test_keypair = Some(keypair);
        Ok(())
    }

    /// Simulate an ETH→SUI bridge transfer.
    ///
    /// Builds and executes the approve+claim transaction that a bridge relayer would submit.
    /// The fork's test committee key (installed by `setup_bridge_test_committee`) is used to
    /// produce the required certificate.
    ///
    /// - `recipient`:     SuiAddress that will receive the bridged tokens.
    /// - `token_id`:      Bridge token ID registered in the mainnet bridge treasury:
    ///                    1=BTC, 2=ETH, 4=USDT, 6=WLBTC. Note: USDC (3) is NOT a
    ///                    native bridge token on Sui mainnet — it uses Circle CCTP.
    /// - `amount`:        Amount in token-native adjusted units.
    /// - `nonce`:         Unique bridge action nonce (must not have been used before).
    /// - `eth_chain_id`:  Source chain (10=EthMainnet, 11=EthSepolia, 12=EthCustom).
    pub fn simulate_eth_to_sui_bridge(
        &mut self,
        recipient: SuiAddress,
        token_id: u8,
        amount: u64,
        nonce: u64,
        eth_chain_id: u8,
    ) -> Result<(TransactionEffects, TransactionEvents)> {
        use std::collections::{BTreeMap, HashMap};
        use move_core_types::language_storage::TypeTag;
        use sui_bridge::crypto::BridgeAuthoritySignInfo;
        use sui_bridge::sui_transaction_builder::build_sui_transaction;
        use sui_bridge::types::{
            BridgeAction, BridgeCommitteeValiditySignInfo, CertifiedBridgeAction,
            EthToSuiBridgeAction, VerifiedCertifiedBridgeAction,
        };
        use sui_bridge::abi::EthToSuiTokenBridgeV1;
        use sui_types::bridge::{
            BridgeChainId, BridgeInnerV1, get_bridge_obj_initial_shared_version, get_bridge_wrapper,
        };
        use sui_types::dynamic_field::{Field, get_dynamic_field_object_from_store};
        use sui_types::transaction::{ObjectArg, SharedObjectMutability};

        // Phase 1: read bridge state (all immutable borrows, results are owned values).
        let (sui_chain_id_u8, token_type_tags) = {
            let wrapper = get_bridge_wrapper(&self.store)
                .map_err(|e| anyhow!("bridge not seeded: {e}"))?;
            let versioned_id: ObjectID = wrapper.version.id.id.bytes;
            let version: u64 = wrapper.version.version;
            let df_obj =
                get_dynamic_field_object_from_store(&self.store, versioned_id, &version)
                    .map_err(|e| anyhow!("failed to get bridge inner: {e}"))?;
            let Data::Move(ref move_obj) = df_obj.data else {
                return Err(anyhow!("bridge inner is not a Move object"));
            };
            let field: Field<u64, BridgeInnerV1> = bcs::from_bytes(move_obj.contents())
                .map_err(|e| anyhow!("deserialize BridgeInnerV1: {e}"))?;
            let mut tags: HashMap<u8, TypeTag> = HashMap::new();
            for entry in &field.value.treasury.id_token_type_map.contents {
                // TypeName stores addresses without the `0x` prefix, but TypeTag::parse
                // requires it — add `0x` if missing before parsing.
                let type_str = if entry.value.starts_with("0x") {
                    entry.value.clone()
                } else {
                    format!("0x{}", entry.value)
                };
                if let Ok(tag) = type_str.parse::<TypeTag>() {
                    tags.insert(entry.key, tag);
                }
            }
            (field.value.chain_id, tags)
        };

        let initial_version = get_bridge_obj_initial_shared_version(&self.store)
            .map_err(|e| anyhow!("bridge lookup failed: {e}"))?
            .ok_or_else(|| anyhow!("bridge object not found — call seed_bridge_objects first"))?;

        let eth_chain = BridgeChainId::try_from(eth_chain_id)
            .map_err(|_| anyhow!("invalid eth_chain_id {eth_chain_id}"))?;
        let sui_chain = BridgeChainId::try_from(sui_chain_id_u8)
            .map_err(|_| anyhow!("unsupported Sui bridge chain_id {sui_chain_id_u8}"))?;

        // Phase 2: sign the certified bridge action (borrows self.bridge_test_keypair).
        let verified_certified = {
            let keypair = self
                .bridge_test_keypair
                .as_ref()
                .ok_or_else(|| anyhow!("call setup_bridge_test_committee first"))?;

            let action = BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
                eth_tx_hash: Default::default(),
                eth_event_index: 0,
                eth_bridge_event: EthToSuiTokenBridgeV1 {
                    nonce,
                    sui_chain_id: sui_chain,
                    eth_chain_id: eth_chain,
                    sui_address: recipient,
                    eth_address: Default::default(),
                    token_id,
                    sui_adjusted_amount: amount,
                },
            });

            let sig = BridgeAuthoritySignInfo::new(&action, keypair);
            let sigs =
                BTreeMap::from([(sig.authority_pub_key_bytes(), sig.signature.clone())]);
            let certified = CertifiedBridgeAction::new_from_data_and_sig(
                action,
                BridgeCommitteeValiditySignInfo { signatures: sigs },
            );
            VerifiedCertifiedBridgeAction::new_from_verified(certified)
        };

        // Phase 3: mutable operations (keypair borrow has ended).
        let bridge_object_arg = ObjectArg::SharedObject {
            id: sui_types::SUI_BRIDGE_OBJECT_ID,
            initial_shared_version: initial_version,
            mutability: SharedObjectMutability::Mutable,
        };

        let gas_coin_id = self.fund_account(recipient, 1_000_000_000)?;
        let gas_obj = ObjectStore::get_object(&self.store, &gas_coin_id)
            .ok_or_else(|| anyhow!("gas coin not found after funding"))?;
        let gas_ref = gas_obj.compute_object_reference();
        let rgp = self.reference_gas_price();

        let tx_data = build_sui_transaction(
            recipient,
            &gas_ref,
            verified_certified,
            bridge_object_arg,
            &token_type_tags,
            rgp,
        )
        .map_err(|e| anyhow!("build_sui_transaction failed: {e}"))?;

        self.execute_transaction(tx_data)
    }

    /// Simulate a SUI→ETH bridge transfer by calling `bridge::send_token<T>`.
    ///
    /// Executes the on-chain Move call that emits the bridge event. An off-chain
    /// relayer would normally observe this event and relay it to Ethereum.
    ///
    /// - `sender`:           Address of the account sending tokens.
    /// - `token_object_id`:  ObjectID of the `Coin<T>` to bridge (must be locally seeded
    ///   and owned by `sender`).
    /// - `eth_chain_id`:     Target chain (10=EthMainnet, 11=EthSepolia, 12=EthCustom).
    /// - `eth_recipient`:    20-byte Ethereum recipient address as raw bytes.
    /// - `gas_budget`:       Gas budget in MIST.
    pub fn simulate_sui_to_eth_bridge(
        &mut self,
        sender: SuiAddress,
        token_object_id: ObjectID,
        eth_chain_id: u8,
        eth_recipient: Vec<u8>,
        gas_budget: u64,
    ) -> Result<(TransactionEffects, TransactionEvents)> {
        use move_core_types::identifier::Identifier;
        use move_core_types::language_storage::TypeTag;
        use sui_types::bridge::get_bridge_obj_initial_shared_version;
        use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
        use sui_types::transaction::{ObjectArg, SharedObjectMutability, TransactionData};

        let token_obj = ObjectStore::get_object(&self.store, &token_object_id).ok_or_else(
            || anyhow!("token object {token_object_id} not found — seed it first"),
        )?;
        let Data::Move(ref move_obj) = token_obj.data else {
            return Err(anyhow!("token object is not a Move object"));
        };
        let coin_type = move_obj.type_().to_string();
        let token_type_str = coin_type
            .strip_prefix("0x2::coin::Coin<")
            .and_then(|s| s.strip_suffix('>'))
            .ok_or_else(|| anyhow!("token object is not a Coin<T>: {coin_type}"))?;
        let type_tag: TypeTag = token_type_str
            .parse()
            .map_err(|e| anyhow!("failed to parse token type '{token_type_str}': {e}"))?;
        let token_ref = token_obj.compute_object_reference();

        let initial_version = get_bridge_obj_initial_shared_version(&self.store)
            .map_err(|e| anyhow!("bridge lookup failed: {e}"))?
            .ok_or_else(|| anyhow!("bridge not found — call seed_bridge_objects first"))?;

        let mut ptb = ProgrammableTransactionBuilder::new();
        let bridge_arg = ptb
            .obj(ObjectArg::SharedObject {
                id: sui_types::SUI_BRIDGE_OBJECT_ID,
                initial_shared_version: initial_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .map_err(|e| anyhow!("bridge obj arg: {e}"))?;
        let chain_id_arg = ptb
            .pure(eth_chain_id)
            .map_err(|e| anyhow!("chain_id arg: {e}"))?;
        let recv_arg = ptb
            .pure(eth_recipient)
            .map_err(|e| anyhow!("recv_addr arg: {e}"))?;
        let coin_arg = ptb
            .obj(ObjectArg::ImmOrOwnedObject(token_ref))
            .map_err(|e| anyhow!("coin obj arg: {e}"))?;
        ptb.programmable_move_call(
            sui_types::BRIDGE_PACKAGE_ID,
            Identifier::new("bridge").map_err(|e| anyhow!("module id: {e}"))?,
            Identifier::new("send_token").map_err(|e| anyhow!("fn id: {e}"))?,
            vec![type_tag],
            vec![bridge_arg, chain_id_arg, recv_arg, coin_arg],
        );

        let gas_coin_id = self.fund_account(sender, gas_budget.saturating_mul(2))?;
        let gas_obj = ObjectStore::get_object(&self.store, &gas_coin_id)
            .ok_or_else(|| anyhow!("gas coin not found after funding"))?;
        let gas_ref = gas_obj.compute_object_reference();
        let rgp = self.reference_gas_price();

        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_ref],
            ptb.finish(),
            gas_budget,
            rgp,
        );
        self.execute_transaction(tx_data)
    }

    /// Re-execute a previously executed transaction against current state.
    /// The results may differ from the original if state has changed — this is intentional,
    /// allowing researchers to test "what if the state was different when this tx ran?"
    pub fn replay_transaction(
        &mut self,
        digest: TransactionDigest,
    ) -> Result<(TransactionEffects, TransactionEvents)> {
        let tx_data = {
            let local = self.store.local.read().unwrap();
            local
                .transactions
                .get(&digest)
                .ok_or_else(|| anyhow!("transaction {digest} not found in local state"))?
                .transaction_data()
                .clone()
        };
        self.execute_transaction(tx_data)
    }
}

/// Start the fork server. This is the main entry point called from the CLI.
pub async fn run(config: ForkConfig) -> Result<()> {
    info!(
        network = %config.node.network_name(),
        checkpoint = ?config.checkpoint,
        port = config.rpc_port,
        "Starting sui fork"
    );

    let node = if let Some(ref path) = config.state_file {
        info!(path = %path.display(), "Loading fork state from file");
        ForkedNode::load_state(path, &config.node)?
    } else {
        bootstrap::bootstrap(&config).await?
    };

    info!(
        fork_checkpoint = node.fork_checkpoint,
        chain_id = %node.chain_id,
        "Fork bootstrapped successfully"
    );

    let node = Arc::new(Mutex::new(node));
    rpc::serve(node, config.rpc_port).await
}
