// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

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
use sui_json_rpc_types::{
    BalanceChange, DevInspectResults, DryRunTransactionBlockResponse, ObjectChange,
    SuiTransactionBlock, SuiTransactionBlockEvents, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::default_hash;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::gas_coin;
use sui_types::object::Object;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
};
use sui_types::signature::GenericSignature;
use sui_types::storage::WriteKind;
use sui_types::sui_serde::BigInt;
use sui_types::transaction::{
    InputObjectKind, Transaction, TransactionData, TransactionDataAPI, TransactionKind,
};
use tracing::instrument;

use crate::api::JsonRpcMetrics;
use crate::api::WriteApiServer;
use crate::error::{Error, SuiRpcInputError, SuiRpcServerResult};
use crate::{
    get_balance_changes_from_effect, get_object_changes, with_tracing, ObjectProviderCache,
    SuiRpcModule,
};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransactionExecutionInternalTrait {
    async fn execute_transaction_block(
        &self,
        tx: Transaction,
        request_type: ExecuteTransactionRequestType,
    ) -> SuiRpcServerResult<ExecuteTransactionResponse>;
    fn show_transaction_details(&self, tx: &Transaction)
        -> SuiRpcServerResult<SuiTransactionBlock>;
    fn show_transaction_events(
        &self,
        tx_digest: TransactionDigest,
        tx_events: TransactionEvents,
    ) -> SuiRpcServerResult<SuiTransactionBlockEvents>;
    fn get_object_cache(
        &self,
        cache: Option<BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>>,
    ) -> ObjectProviderCache<Arc<AuthorityState>>;
    async fn get_object_changes(
        &self,
        object_cache: &ObjectProviderCache<Arc<AuthorityState>>,
        sender: SuiAddress,
        effects: &TransactionEffects,
    ) -> SuiRpcServerResult<Vec<ObjectChange>>;
    async fn get_balance_changes(
        &self,
        object_cache: &ObjectProviderCache<Arc<AuthorityState>>,
        effects: &TransactionEffects,
        input_objs: Vec<InputObjectKind>,
        gas_coin: Option<ObjectID>,
    ) -> SuiRpcServerResult<Vec<BalanceChange>>;
    fn get_metrics(&self) -> Arc<JsonRpcMetrics>;

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        _epoch: Option<BigInt<u64>>,
    ) -> SuiRpcServerResult<DevInspectResults>;

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> SuiRpcServerResult<DryRunTransactionBlockResponse>;
}

pub struct TransactionExecutionInternal {
    state: Arc<AuthorityState>,
    transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
    metrics: Arc<JsonRpcMetrics>,
}

impl TransactionExecutionInternal {
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
}

#[async_trait]
impl TransactionExecutionInternalTrait for TransactionExecutionInternal {
    fn get_metrics(&self) -> Arc<JsonRpcMetrics> {
        self.metrics.clone()
    }

    fn get_object_cache(
        &self,
        cache: Option<BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)>>,
    ) -> ObjectProviderCache<Arc<AuthorityState>> {
        match cache {
            Some(cache_value) => {
                ObjectProviderCache::new_with_cache(self.state.clone(), cache_value)
            }
            None => ObjectProviderCache::new(self.state.clone()),
        }
    }

    fn show_transaction_details(
        &self,
        tx: &Transaction,
    ) -> SuiRpcServerResult<SuiTransactionBlock> {
        let epoch_store = self.state.load_epoch_store_one_call_per_task(); // internal
        Ok(SuiTransactionBlock::try_from(
            tx.data().clone(),
            epoch_store.module_cache(),
        )?)
    }

    fn show_transaction_events(
        &self,
        tx_digest: TransactionDigest,
        tx_events: TransactionEvents,
    ) -> SuiRpcServerResult<SuiTransactionBlockEvents> {
        let module_cache = self
            .state
            .load_epoch_store_one_call_per_task()
            .module_cache()
            .clone();
        Ok(SuiTransactionBlockEvents::try_from(
            tx_events,
            tx_digest,
            None,
            module_cache.as_ref(),
        )?)
    }

    async fn get_balance_changes(
        &self,
        object_cache: &ObjectProviderCache<Arc<AuthorityState>>,
        effects: &TransactionEffects,
        input_objs: Vec<InputObjectKind>,
        gas_coin: Option<ObjectID>,
    ) -> SuiRpcServerResult<Vec<BalanceChange>> {
        Ok(get_balance_changes_from_effect(object_cache, effects, input_objs, gas_coin).await?)
    }

    async fn get_object_changes(
        &self,
        object_cache: &ObjectProviderCache<Arc<AuthorityState>>,
        sender: SuiAddress,
        effects: &TransactionEffects,
    ) -> SuiRpcServerResult<Vec<ObjectChange>> {
        Ok(get_object_changes(
            object_cache,
            sender,
            effects.modified_at_versions(),
            effects.all_changed_objects(),
            effects.all_deleted(),
        )
        .await?)
    }

    async fn execute_transaction_block(
        &self,
        tx: Transaction,
        request_type: ExecuteTransactionRequestType,
    ) -> SuiRpcServerResult<ExecuteTransactionResponse> {
        let transaction_orchestrator = self.transaction_orchestrator.clone(); // internal
        let orch_timer = self.metrics.orchestrator_latency_ms.start_timer(); // internal
        let response = spawn_monitored_task!(transaction_orchestrator.execute_transaction_block(
            ExecuteTransactionRequest {
                transaction: tx,
                request_type,
            }
        ))
        .await?
        .map_err(Error::from)?;
        drop(orch_timer);
        Ok(response)
    }

    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        _epoch: Option<BigInt<u64>>,
    ) -> SuiRpcServerResult<DevInspectResults> {
        let tx_kind: TransactionKind = bcs::from_bytes(&tx_bytes.to_vec()?)?;
        Ok(self
            .state
            .dev_inspect_transaction_block(sender_address, tx_kind, gas_price.map(|i| *i))
            .await?)
    }

    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> SuiRpcServerResult<DryRunTransactionBlockResponse> {
        let (txn_data, txn_digest) = get_transaction_data_and_digest(tx_bytes)?;
        let input_objs = txn_data.input_objects()?;
        let sender = txn_data.sender();
        let (resp, written_objects, transaction_effects, mock_gas) = self
            .state
            .dry_exec_transaction(txn_data.clone(), txn_digest)
            .await?;
        let object_cache = self.get_object_cache(Some(written_objects));
        let balance_changes = self
            .get_balance_changes(&object_cache, &transaction_effects, input_objs, mock_gas)
            .await?;
        let object_changes = self
            .get_object_changes(&object_cache, sender, &transaction_effects)
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

pub struct TransactionExecutionApi {
    internal: Arc<dyn TransactionExecutionInternalTrait + Send + Sync>,
}

impl TransactionExecutionApi {
    pub fn new(
        state: Arc<AuthorityState>,
        transaction_orchestrator: Arc<TransactiondOrchestrator<NetworkAuthorityClient>>,
        metrics: Arc<JsonRpcMetrics>,
    ) -> Self {
        Self {
            internal: Arc::new(TransactionExecutionInternal::new(
                state,
                transaction_orchestrator,
                metrics,
            )),
        }
    }

    async fn execute_transaction_block_internal(
        &self,
        tx_bytes: Base64,
        signatures: Vec<Base64>,
        opts: Option<SuiTransactionBlockResponseOptions>,
        request_type: Option<ExecuteTransactionRequestType>,
    ) -> SuiRpcServerResult<SuiTransactionBlockResponse> {
        let opts = opts.unwrap_or_default();
        let request_type = match (request_type, opts.require_local_execution()) {
            (Some(ExecuteTransactionRequestType::WaitForEffectsCert), true) => {
                return Err(Error::SuiRpcInputError(
                    SuiRpcInputError::InvalidExecuteTransactionRequestType,
                ));
            }
            (t, _) => t.unwrap_or_else(|| opts.default_execution_request_type()),
        };
        let tx_data: TransactionData = bcs::from_bytes(&tx_bytes.to_vec()?)?;
        let sender = tx_data.sender();
        let input_objs = tx_data.input_objects().unwrap_or_default();

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(GenericSignature::from_bytes(&sig.to_vec()?)?);
        }
        let tx = Transaction::from_generic_sig_data(tx_data, Intent::sui_transaction(), sigs);
        let digest = *tx.digest();
        let raw_transaction = if opts.show_raw_input {
            bcs::to_bytes(tx.data())?
        } else {
            vec![]
        };

        // Needs to be before the actual execution, as tx does not implement copy
        let transaction = opts
            .show_input
            .then_some(self.internal.show_transaction_details(&tx)?);

        let response = self
            .internal
            .execute_transaction_block(tx, request_type)
            .await?;

        // build per the transaction options
        let ExecuteTransactionResponse::EffectsCert(cert) = response;
        let (effects, transaction_events, is_executed_locally) = *cert;

        let metrics = self.internal.get_metrics();
        let _post_orch_timer = metrics.post_orchestrator_latency_ms.start_timer();

        let events = opts.show_events.then_some(
            self.internal
                .show_transaction_events(digest, transaction_events)?,
        );

        // balance and object changes
        let object_cache = self.internal.get_object_cache(None);

        let balance_changes = if opts.show_balance_changes && is_executed_locally {
            Some(
                self.internal
                    .get_balance_changes(&object_cache, &effects.effects, input_objs, None)
                    .await?,
            )
        } else {
            None
        };
        let object_changes = if opts.show_object_changes && is_executed_locally {
            Some(
                self.internal
                    .get_object_changes(&object_cache, sender, &effects.effects)
                    .await?,
            )
        } else {
            None
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
        with_tracing!(async move {
            Ok(self
                .execute_transaction_block_internal(tx_bytes, signatures, opts, request_type)
                .await?)
        })
    }

    #[instrument(skip(self))]
    async fn dev_inspect_transaction_block(
        &self,
        sender_address: SuiAddress,
        tx_bytes: Base64,
        gas_price: Option<BigInt<u64>>,
        _epoch: Option<BigInt<u64>>,
    ) -> RpcResult<DevInspectResults> {
        with_tracing!(async move {
            Ok(self
                .internal
                .dev_inspect_transaction_block(sender_address, tx_bytes, gas_price, _epoch)
                .await?)
        })
    }

    #[instrument(skip(self))]
    async fn dry_run_transaction_block(
        &self,
        tx_bytes: Base64,
    ) -> RpcResult<DryRunTransactionBlockResponse> {
        with_tracing!(async move { Ok(self.internal.dry_run_transaction_block(tx_bytes).await?) })
    }
}

impl SuiRpcModule for TransactionExecutionApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::WriteApiOpenRpc::module_doc()
    }
}

fn get_transaction_data_and_digest(
    tx_bytes: Base64,
) -> SuiRpcServerResult<(TransactionData, TransactionDigest)> {
    let tx_data = bcs::from_bytes(&tx_bytes.to_vec()?)?;
    let intent_msg = IntentMessage::new(
        Intent {
            version: IntentVersion::V0,
            scope: IntentScope::TransactionData,
            app_id: AppId::Sui,
        },
        tx_data,
    );
    let txn_digest = TransactionDigest::new(default_hash(&intent_msg.value));
    Ok((intent_msg.value, txn_digest))
}

#[cfg(test)]
mod tests {
    mod execute_transaction_block_tests {
        use super::super::*;
        use jsonrpsee::types::ErrorObjectOwned;
        use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};
        use sui_json_rpc_types::{
            ObjectChange, OwnedObjectRef, SuiExecutionStatus, SuiObjectRef,
            SuiTransactionBlockData, SuiTransactionBlockEffects, SuiTransactionBlockEffectsV1,
            TransactionBlockBytes,
        };
        use sui_types::{
            base_types::{ObjectID, SequenceNumber},
            crypto::{get_key_pair_from_rng, AccountKeyPair},
            digests::{ObjectDigest, TransactionEventsDigest},
            gas::GasCostSummary,
            object::Owner,
            parse_sui_struct_tag,
            transaction::TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
            utils::to_sender_signed_transaction,
        };

        fn mock_transaction_data() -> (
            TransactionBlockBytes,
            TransactionData,
            Vec<GenericSignature>,
            SuiAddress,
            ObjectID,
            SuiTransactionBlockResponse,
        ) {
            let mut rng = StdRng::from_seed([0; 32]);
            let (signer, kp): (_, AccountKeyPair) = get_key_pair_from_rng(&mut rng);
            let recipient = SuiAddress::from(ObjectID::new(rng.gen()));
            let obj_id = ObjectID::new(rng.gen());
            let gas_ref = (
                ObjectID::new(rng.gen()),
                SequenceNumber::from_u64(2),
                ObjectDigest::new(rng.gen()),
            );
            let object_ref = (
                obj_id,
                SequenceNumber::from_u64(2),
                ObjectDigest::new(rng.gen()),
            );

            let data = TransactionData::new_transfer(
                recipient,
                object_ref,
                signer,
                gas_ref,
                TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
                10,
            );
            let data1 = data.clone();
            let data2 = data.clone();
            let data3 = data.clone();

            let tx = to_sender_signed_transaction(data, &kp);
            let tx1 = tx.clone();
            let signatures = tx.into_inner().tx_signatures().to_vec();
            let raw_transaction = bcs::to_bytes(tx1.data()).unwrap();

            let tx_digest = tx1.digest();
            let object_change = ObjectChange::Transferred {
                sender: signer,
                recipient: Owner::AddressOwner(recipient),
                object_type: parse_sui_struct_tag("0x2::example::Object").unwrap(),
                object_id: object_ref.0,
                version: object_ref.1,
                digest: ObjectDigest::new(rng.gen()),
            };
            struct NoOpsModuleResolver;
            impl ModuleResolver for NoOpsModuleResolver {
                type Error = Error;
                fn get_module(&self, _id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
                    Ok(None)
                }
            }
            let result = SuiTransactionBlockResponse {
                digest: *tx_digest,
                effects: Some(SuiTransactionBlockEffects::V1(
                    SuiTransactionBlockEffectsV1 {
                        status: SuiExecutionStatus::Success,
                        executed_epoch: 0,
                        modified_at_versions: vec![],
                        gas_used: GasCostSummary {
                            computation_cost: 100,
                            storage_cost: 100,
                            storage_rebate: 10,
                            non_refundable_storage_fee: 0,
                        },
                        shared_objects: vec![],
                        transaction_digest: TransactionDigest::new(rng.gen()),
                        created: vec![],
                        mutated: vec![
                            OwnedObjectRef {
                                owner: Owner::AddressOwner(signer),
                                reference: gas_ref.into(),
                            },
                            OwnedObjectRef {
                                owner: Owner::AddressOwner(recipient),
                                reference: object_ref.into(),
                            },
                        ],
                        unwrapped: vec![],
                        deleted: vec![],
                        unwrapped_then_deleted: vec![],
                        wrapped: vec![],
                        gas_object: OwnedObjectRef {
                            owner: Owner::ObjectOwner(signer),
                            reference: SuiObjectRef::from(gas_ref),
                        },
                        events_digest: Some(TransactionEventsDigest::new(rng.gen())),
                        dependencies: vec![],
                    },
                )),
                events: None,
                object_changes: Some(vec![object_change]),
                balance_changes: None,
                timestamp_ms: None,
                transaction: Some(SuiTransactionBlock {
                    data: SuiTransactionBlockData::try_from(data1, &&mut NoOpsModuleResolver)
                        .unwrap(),
                    tx_signatures: signatures.clone(),
                }),
                raw_transaction,
                confirmed_local_execution: None,
                checkpoint: None,
                errors: vec![],
            };
            let tx_bytes = TransactionBlockBytes::from_data(data3).unwrap();

            (tx_bytes, data2, signatures, recipient, obj_id, result)
        }

        #[tokio::test]
        async fn test_invalid_execute_transaction_request_type() {
            let (tx_bytes, _, signatures, _, _, _) = mock_transaction_data();
            let mock_internal = MockTransactionExecutionInternalTrait::new();
            let opts = SuiTransactionBlockResponseOptions::new().with_balance_changes();
            let request_type = ExecuteTransactionRequestType::WaitForEffectsCert;
            let transaction_execution_api = TransactionExecutionApi {
                internal: Arc::new(mock_internal),
            };

            let response = transaction_execution_api
                .execute_transaction_block(
                    tx_bytes.tx_bytes,
                    signatures
                        .into_iter()
                        .map(|s| Base64::from_bytes(s.as_ref()))
                        .collect(),
                    Some(opts),
                    Some(request_type),
                )
                .await;
            let error_result = response.unwrap_err();
            let error_object: ErrorObjectOwned = error_result.into();

            assert_eq!(error_object.code(), -32602);
            assert_eq!(error_object.message(), "request_type` must set to `None` or `WaitForLocalExecution` if effects is required in the response");
        }
    }
}
