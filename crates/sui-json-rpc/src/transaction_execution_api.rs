// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fastcrypto::encoding::Base64;
use fastcrypto::traits::ToFromBytes;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use mysten_metrics::spawn_monitored_task;
use shared_crypto::intent::{AppId, Intent, IntentMessage, IntentScope, IntentVersion};
use sui_core::authority::AuthorityState;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_api::{JsonRpcMetrics, WriteApiOpenRpc, WriteApiServer};
use sui_json_rpc_types::{
    DevInspectArgs, DevInspectResults, DryRunTransactionBlockResponse, SuiTransactionBlock,
    SuiTransactionBlockEvents, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::default_hash;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
};
use sui_types::signature::GenericSignature;
use sui_types::sui_serde::BigInt;
use sui_types::transaction::{
    InputObjectKind, Transaction, TransactionData, TransactionDataAPI, TransactionKind,
};
use tracing::instrument;

use crate::authority_state::StateRead;
use crate::error::{Error, SuiRpcInputError};
use crate::{
    get_balance_changes_from_effect, get_object_changes, with_tracing, ObjectProviderCache,
    SuiRpcModule,
};

pub struct TransactionExecutionApi {
    state: Arc<dyn StateRead>,
    transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    metrics: Arc<JsonRpcMetrics>,
}

impl TransactionExecutionApi {
    pub fn new(
        state: Arc<AuthorityState>,
        transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
        metrics: Arc<JsonRpcMetrics>,
    ) -> Self {
        Self {
            state,
            transaction_orchestrator,
            metrics,
        }
    }

    pub fn convert_bytes<T: serde::de::DeserializeOwned>(
        &self,
        tx_bytes: Base64,
    ) -> Result<T, SuiRpcInputError> {
        let data: T = bcs::from_bytes(&tx_bytes.to_vec()?)?;
        Ok(data)
    }

    #[allow(clippy::type_complexity)]
    fn prepare_execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<
        (
            SuiTransactionBlockResponseOptions,
            ExecuteTransactionRequestType,
            SuiAddress,
            Vec<InputObjectKind>,
            Transaction,
            Option<SuiTransactionBlock>,
            Vec<u8>,
        ),
        SuiRpcInputError,
    > {
        let opts = opts.unwrap_or_default();
        let request_type = match (request_type, opts.require_local_execution()) {
            (Some(ExecuteTransactionRequestType::WaitForEffectsCert), true) => {
                Err(SuiRpcInputError::InvalidExecuteTransactionRequestType)?
            }
            (t, _) => t.unwrap_or_else(|| opts.default_execution_request_type()),
        };
        let tx_data: TransactionData = self.convert_bytes(tx_bytes)?;
        let sender = tx_data.sender();
        let input_objs = tx_data.input_objects().unwrap_or_default();

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(GenericSignature::from_bytes(&sig.to_vec()?)?);
        }
        let txn = Transaction::from_generic_sig_data(tx_data, sigs);
        let raw_transaction = if opts.show_raw_input {
            bcs::to_bytes(txn.data())?
        } else {
            vec![]
        };
        let transaction = if opts.show_input {
            let epoch_store = self.state.load_epoch_store_one_call_per_task();
            Some(SuiTransactionBlock::try_from(
                txn.data().clone(),
                epoch_store.module_cache(),
            )?)
        } else {
            None
        };
        Ok((
            opts,
            request_type,
            sender,
            input_objs,
            txn,
            transaction,
            raw_transaction,
        ))
    }

    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> Result<SuiTransactionBlockResponse, Error> {
        let (opts, request_type, sender, input_objs, txn, transaction, raw_transaction) =
            self.prepare_execute_transaction_block(tx_bytes, signatures, opts, request_type)?;
        let digest = *txn.digest();

        let transaction_orchestrator = self.transaction_orchestrator.clone();
        let orch_timer = self.metrics.orchestrator_latency_ms.start_timer();
        let response = spawn_monitored_task!(transaction_orchestrator.execute_transaction_block(
            ExecuteTransactionRequest {
                transaction: txn,
                request_type,
            }
        ))
        .await?
        .map_err(Error::from)?;
        drop(orch_timer);

        let _post_orch_timer = self.metrics.post_orchestrator_latency_ms.start_timer();
        let ExecuteTransactionResponse::EffectsCert(cert) = response;
        let (effects, transaction_events, is_executed_locally) = *cert;
        let mut events: Option<SuiTransactionBlockEvents> = None;
        if opts.show_events {
            let epoch_store = self.state.load_epoch_store_one_call_per_task();
            let backing_package_store = self.state.get_backing_package_store();
            let mut layout_resolver = epoch_store
                .executor()
                .type_layout_resolver(Box::new(backing_package_store.as_ref()));
            events = Some(SuiTransactionBlockEvents::try_from(
                transaction_events,
                digest,
                None,
                layout_resolver.as_mut(),
            )?);
        }

        let object_cache = ObjectProviderCache::new(self.state.clone());
        let balance_changes = if opts.show_balance_changes && is_executed_locally {
            Some(
                get_balance_changes_from_effect(&object_cache, &effects.effects, input_objs, None)
                    .await?,
            )
        } else {
            None
        };
        let object_changes = if opts.show_object_changes && is_executed_locally {
            Some(
                get_object_changes(
                    &object_cache,
                    sender,
                    effects.effects.modified_at_versions(),
                    effects.effects.all_changed_objects(),
                    effects.effects.all_removed_objects(),
                )
                .await?,
            )
        } else {
            None
        };

        let raw_effects = if opts.show_raw_effects {
            bcs::to_bytes(&effects.effects)?
        } else {
            vec![]
        };

        Ok(SuiTransactionBlockResponse {
            digest,
            transaction,
            raw_transaction,
            effects: opts.show_effects.then_some(effects.effects.try_into()?),
            events,
            object_changes,
            balance_changes,
            timestamp_ms: None,
            confirmed_local_execution: Some(is_executed_locally),
            checkpoint: None,
            errors: vec![],
            raw_effects,
        })
    }

    pub fn prepare_dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> Result<(TransactionData, TransactionDigest, Vec<InputObjectKind>), SuiRpcInputError> {
        let tx_data: TransactionData = self.convert_bytes(tx_bytes)?;
        let input_objs = tx_data.input_objects()?;
        let intent_msg = IntentMessage::new(
            Intent {
                version: IntentVersion::V0,
                scope: IntentScope::TransactionData,
                app_id: AppId::Sui,
            },
            tx_data,
        );
        let txn_digest = TransactionDigest::new(default_hash(&intent_msg.value));
        Ok((intent_msg.value, txn_digest, input_objs))
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> Result<DryRunTransactionBlockResponse, Error> {
        let (txn_data, txn_digest, input_objs) =
            self.prepare_dry_run_transaction_block(tx_bytes)?;
        let sender = txn_data.sender();
        let (resp, written_objects, transaction_effects, mock_gas) = self
            .state
            .dry_exec_transaction(txn_data.clone(), txn_digest)
            .await?;
        let object_cache = ObjectProviderCache::new_with_cache(self.state.clone(), written_objects);
        let balance_changes = get_balance_changes_from_effect(
            &object_cache,
            &transaction_effects,
            input_objs,
            mock_gas,
        )
        .await?;
        let object_changes = get_object_changes(
            &object_cache,
            sender,
            transaction_effects.modified_at_versions(),
            transaction_effects.all_changed_objects(),
            transaction_effects.all_removed_objects(),
        )
        .await?;

        Ok(DryRunTransactionBlockResponse {
            effects: resp.effects,
            events: resp.events,
            object_changes,
            balance_changes,
            input: resp.input,
        })
    }
}

#[async_trait]
impl WriteApiServer for TransactionExecutionApi {
    #[instrument(skip(self))]
    async fn execute_transaction_block(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> RpcResult<SuiTransactionBlockResponse> {
        with_tracing!(Duration::from_secs(10), async move {
            self.execute_transaction_block(tx_bytes, signatures, opts, request_type)
                .await
        })
    }

    #[instrument(skip(self))]
    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        _epoch: Option<BigInt<u64>>,
        additional_args: Option<DevInspectArgs>,
    ) -> RpcResult<DevInspectResults> {
        with_tracing!(async move {
            let DevInspectArgs {
                gas_sponsor,
                gas_budget,
                gas_objects,
                show_raw_txn_data_and_effects,
                skip_checks,
            } = additional_args.unwrap_or_default();
            let tx_kind: TransactionKind = self.convert_bytes(tx_bytes)?;
            self.state
                .dev_inspect_transaction_block(
                    sender_address,
                    tx_kind,
                    gas_price.map(|i| *i),
                    gas_budget.map(|i| *i),
                    gas_sponsor,
                    gas_objects,
                    show_raw_txn_data_and_effects,
                    skip_checks,
                )
                .await
                .map_err(Error::from)
        })
    }

    #[instrument(skip(self))]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        with_tracing!(async move { self.dry_run_transaction_block(tx_bytes).await })
    }
}

impl SuiRpcModule for TransactionExecutionApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        WriteApiOpenRpc::module_doc()
    }
}
