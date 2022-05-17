// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::{
    collections::HashSet,
    sync::{atomic::AtomicU64, Arc},
};
use sui_config::genesis::Genesis;

use crate::{
    authority::{AuthorityTemporaryStore, ReplicaStore},
    gateway_state::GatewayTxSeqNumber,
    gateway_types::TransactionEffectsResponse,
};
use move_binary_format::CompiledModule;
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunctionTable};
use sui_adapter::adapter;
use sui_types::{
    base_types::{ObjectID, TransactionDigest, TxContext},
    committee::Committee as SuiCommittee,
    error::{SuiError, SuiResult},
    fp_ensure,
    gas::SuiGasStatus,
    messages::Transaction,
    object::Object,
    MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS,
};
use tracing::debug;

// use std::path::Path;

// use crate::{
//     api::{RpcGatewayServer, TransactionBytes},
//     rpc_gateway::responses::{ObjectResponse, SuiTypeTag},
// };
// use anyhow::anyhow;
// use async_trait::async_trait;
// use jsonrpsee::core::RpcResult;
// use sui_core::gateway_types::{TransactionEffectsResponse, TransactionResponse};

// use sui_core::gateway_state::GatewayTxSeqNumber;
// use sui_core::gateway_types::GetObjectInfoResponse;
// use sui_core::sui_json::SuiJsonValue;
// use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
// use sui_types::sui_serde::Base64;

const MAX_TX_RANGE_SIZE: u64 = 4096;

pub struct FullNodeState {
    pub store: Arc<ReplicaStore>,
    pub committee: SuiCommittee,
    pub next_tx_seq_number: AtomicU64,

    /// Move native functions that are available to invoke
    _native_functions: NativeFunctionTable,
    /// Will be used for local exec in future
    _move_vm: Arc<MoveVM>,
}

impl FullNodeState {
    pub async fn new_without_genesis(
        committee: SuiCommittee,
        store: Arc<ReplicaStore>,
    ) -> Result<Self, SuiError> {
        let native_functions =
            sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
        let next_tx_seq_number = AtomicU64::new(store.next_sequence_number()?);
        Ok(Self {
            committee,
            store,
            _native_functions: native_functions.clone(),
            _move_vm: Arc::new(
                adapter::new_move_vm(native_functions)
                    .expect("We defined natives to not fail here"),
            ),
            next_tx_seq_number,
        })
    }

    pub async fn new_with_genesis(
        committee: SuiCommittee,
        store: Arc<ReplicaStore>,
        genesis: &Genesis,
    ) -> Result<Self, SuiError> {
        let state = Self::new_without_genesis(committee, store.clone()).await?;

        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail.")
        {
            let mut genesis_ctx = genesis.genesis_ctx().to_owned();
            for genesis_modules in genesis.modules() {
                state
                    .store_package_and_init_modules_for_genesis(
                        &mut genesis_ctx,
                        genesis_modules.to_owned(),
                    )
                    .await
                    .expect("We expect publishing the Genesis packages to not fail");
                state
                    .insert_genesis_objects_bulk_unsafe(
                        &genesis.objects().iter().collect::<Vec<_>>(),
                    )
                    .await;
            }
        }

        Ok(state)
    }

    /// TODO: consolidate with Authoritycounterpart
    /// Persist the Genesis package to DB along with the side effects for module initialization
    async fn store_package_and_init_modules_for_genesis(
        &self,
        ctx: &mut TxContext,
        modules: Vec<CompiledModule>,
    ) -> SuiResult {
        let inputs = Transaction::input_objects_in_compiled_modules(&modules);
        let ids: Vec<_> = inputs.iter().map(|kind| kind.object_id()).collect();
        let input_objects = self.get_objects(&ids[..]).await?;
        // When publishing genesis packages, since the std framework packages all have
        // non-zero addresses, [`Transaction::input_objects_in_compiled_modules`] will consider
        // them as dependencies even though they are not. Hence input_objects contain objects
        // that don't exist on-chain because they are yet to be published.
        #[cfg(debug_assertions)]
        {
            let to_be_published_addresses: HashSet<_> = modules
                .iter()
                .map(|module| *module.self_id().address())
                .collect();
            assert!(
                // An object either exists on-chain, or is one of the packages to be published.
                inputs
                    .iter()
                    .zip(input_objects.iter())
                    .all(|(kind, obj_opt)| obj_opt.is_some()
                        || to_be_published_addresses.contains(&kind.object_id()))
            );
        }
        let filtered = inputs
            .into_iter()
            .zip(input_objects.into_iter())
            .filter_map(|(input, object_opt)| object_opt.map(|object| (input, object)))
            .collect::<Vec<_>>();

        debug_assert!(ctx.digest() == TransactionDigest::genesis());
        let mut temporary_store =
            AuthorityTemporaryStore::new(self.store.clone(), filtered, ctx.digest());
        let package_id = ObjectID::from(*modules[0].self_id().address());
        let natives = self._native_functions.clone();
        let mut gas_status = SuiGasStatus::new_unmetered();
        let vm = adapter::verify_and_link(
            &temporary_store,
            &modules,
            package_id,
            natives,
            &mut gas_status,
        )?;
        adapter::store_package_and_init_modules(
            &mut temporary_store,
            &vm,
            modules,
            ctx,
            &mut gas_status,
        )?;
        self.store
            .update_objects_state_for_genesis(temporary_store, ctx.digest())
            .await
    }

    pub async fn insert_genesis_objects_bulk_unsafe(&self, objects: &[&Object]) {
        self.store
            .bulk_object_insert(objects)
            .await
            .expect("TODO: propagate the error")
    }

    pub fn get_total_transaction_number(&self) -> Result<u64, anyhow::Error> {
        Ok(self.store.next_sequence_number()?)
    }

    pub fn get_transactions_in_range(
        &self,
        start: GatewayTxSeqNumber,
        end: GatewayTxSeqNumber,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            start <= end,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "start must not exceed end, (start={}, end={}) given",
                    start, end
                ),
            }
            .into()
        );
        fp_ensure!(
            end - start <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE,
                    end - start
                ),
            }
            .into()
        );
        let res = self.store.transactions_in_seq_range(start, end)?;
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    pub fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(GatewayTxSeqNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            count <= MAX_TX_RANGE_SIZE,
            SuiError::GatewayInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE, count
                ),
            }
            .into()
        );
        let end = self.get_total_transaction_number()?;
        let start = if end >= count { end - count } else { 0 };
        self.get_transactions_in_range(start, end)
    }

    pub async fn get_objects(
        &self,
        _objects: &[ObjectID],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        self.store.get_objects(_objects)
    }
    pub async fn get_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<TransactionEffectsResponse, anyhow::Error> {
        let opt = self.store.get_certified_transaction(&digest)?;
        match opt {
            Some(certificate) => Ok(TransactionEffectsResponse {
                certificate: certificate.try_into()?,
                effects: self.store.get_effects(&digest)?.into(),
            }),
            None => Err(anyhow!(SuiError::TransactionNotFound { digest })),
        }
    }
}
