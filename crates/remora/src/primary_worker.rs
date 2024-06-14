use core::panic;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_single_node_benchmark::benchmark_context::BenchmarkContext;
use sui_single_node_benchmark::mock_storage::InMemoryObjectStore;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::object::Object;
use sui_types::storage::ObjectStore;
use sui_types::sui_system_state::SuiSystemStateTrait;
use sui_types::transaction::{
    CertifiedTransaction, InputObjectKind, TransactionDataAPI, VerifiedCertificate,
};
use tokio::sync::mpsc;

use super::types::*;
use tokio::time::Duration;

/*****************************************************************************************
 *                                    Primary Worker                                   *
 *****************************************************************************************/

pub struct PrimaryWorkerState {
    pub memory_store: Arc<InMemoryObjectStore>,
    pub context: Arc<BenchmarkContext>,
    pub ready_txs: DashMap<TransactionDigest, ()>,
    pub waiting_child_objs: DashMap<TransactionDigest, HashSet<ObjectID>>,
    pub received_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub received_child_objs: DashMap<TransactionDigest, Vec<Option<(ObjectRef, Object)>>>,
    pub locked_exec_count: DashMap<TransactionDigest, u8>,
    pub genesis_digest: CheckpointDigest,
    pub pending_transactions: Vec<TransactionWithEffects>,
}

impl PrimaryWorkerState {
    pub fn new(
        new_store: InMemoryObjectStore,
        genesis_digest: CheckpointDigest,
        ctx: Arc<BenchmarkContext>,
    ) -> Self {
        Self {
            memory_store: Arc::new(new_store),
            context: ctx,
            ready_txs: DashMap::new(),
            waiting_child_objs: DashMap::new(),
            received_objs: DashMap::new(),
            received_child_objs: DashMap::new(),
            locked_exec_count: DashMap::new(),
            genesis_digest,
            pending_transactions: Vec::new(),
        }
    }

    async fn async_exec(
        full_tx: TransactionWithEffects,
        memstore: Arc<InMemoryObjectStore>,
        protocol_config: &ProtocolConfig,
        reference_gas_price: u64,
        ctx: Arc<BenchmarkContext>,
    ) {
        let tx = full_tx.tx.clone();
        let input_objects = tx.transaction_data().input_objects().unwrap();
        // FIXME: ugly deref
        let objects = memstore
            .read_objects_for_execution(
                &**(ctx.validator().get_epoch_store()),
                &tx.key(),
                &input_objects,
            )
            .unwrap();

        let executable = VerifiedExecutableTransaction::new_from_certificate(
            VerifiedCertificate::new_unchecked(tx),
        );

        let _validator = ctx.validator();
        let (gas_status, input_objects) = sui_transaction_checks::check_certificate_input(
            &executable,
            objects,
            protocol_config,
            reference_gas_price,
        )
        .unwrap();
        let (kind, signer, gas) = executable.transaction_data().execution_parts();
        let (inner_temp_store, _, effects, _) = ctx
            .validator()
            .get_epoch_store()
            .executor()
            .execute_transaction_to_effects(
                &memstore,
                protocol_config,
                ctx.validator()
                    .get_validator()
                    .metrics
                    .limits_metrics
                    .clone(),
                false,
                &HashSet::new(),
                &ctx.validator().get_epoch_store().epoch(),
                0,
                input_objects,
                gas,
                gas_status,
                kind,
                signer,
                *executable.digest(),
            );
        assert!(effects.status().is_ok());
        memstore.commit_objects(inner_temp_store);
        println!("finish exec a txn");
    }

    // Helper: Returns Input objects by reading from the memory_store
    async fn read_input_objects_from_store(
        memory_store: Arc<InMemoryObjectStore>,
        tx: &CertifiedTransaction,
    ) -> HashMap<ObjectID, ObjectRef> {
        let tx_data = tx.transaction_data();
        let input_object_kinds = tx_data
            .input_objects()
            .expect("Cannot get input object kinds");

        let mut input_object_data = Vec::new();
        for kind in &input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id)
                | InputObjectKind::SharedMoveObject { id, .. }
                | InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => {
                    memory_store.get_object(&id).unwrap().unwrap()
                }
            };
            input_object_data.push(obj);
        }

        let mut res = HashMap::new();
        let _ = input_object_data
            .into_iter()
            .map(|obj| res.insert(obj.id(), obj.compute_object_reference()));
        res
    }

    // Triggered every time receiving signal from consensus
    async fn main_run_inner(
        memstore: Arc<InMemoryObjectStore>,
        context: Arc<BenchmarkContext>,
        pending_txns: Vec<TransactionWithEffects>,
        pre_exec_res: Arc<DashMap<TransactionDigest, TransactionWithResults>>,
    ) {
        for full_tx in pending_txns {
            let txid = full_tx.tx.digest();
            let mut skip = true;

            // read the current state from memory_store
            let init_state =
                Self::read_input_objects_from_store(memstore.clone(), &full_tx.tx).await;

            // check if the stale state where pre-exec occurs matches
            if let tx_result = pre_exec_res.get(txid).unwrap() {
                //.map(|r| r.clone()) {
                let TransactionEffects::V1(ref tx_effect) = tx_result.tx_effects else {
                    todo!()
                };
                for (id, vid) in tx_effect.modified_at_versions() {
                    let (_, v, _) = *init_state.get(&id).unwrap();
                    if v != vid {
                        skip = false;
                    }
                }
                if skip {
                    // apply the effect directly
                    memstore
                        .commit_effects(tx_result.tx_effects.clone(), tx_result.written.clone());
                }
            }

            if !skip {
                // re-run the transaction
                // FIXME: need to track dependency btw apply-effects and async_exec
                // so that the effect is visible to the next txn which has
                // overlapping objects (inter-dependency)
                {
                    Self::async_exec(
                        full_tx.clone(),
                        memstore.clone(),
                        context.validator().get_epoch_store().protocol_config(),
                        context.validator().get_epoch_store().reference_gas_price(),
                        context.clone(),
                    )
                    .await
                }
            }
        }
    }

    pub async fn run(
        &mut self,
        _tx_count: u64,
        _duration: Duration,
        in_traffic_manager: &mut mpsc::UnboundedReceiver<RemoraMessage>,
        in_consensus: &mut mpsc::UnboundedReceiver<Vec<TransactionWithEffects>>,
        _out_channel: &mpsc::Sender<NetworkMessage>,
        _my_id: u16,
    ) {
        let pre_exec_res: Arc<DashMap<TransactionDigest, TransactionWithResults>> =
            Arc::new(DashMap::new());

        loop {
            tokio::select! {
                // Receive signal from finished consensus
                Some(msg) = in_consensus.recv() => {
                    println!("Primary worker receive from the consensus engine");

                    // receive a stream of sequenced txn from consensus until the channel is empty
                    self.pending_transactions = msg;
                    println!("PRI recv from consensus channel done");


                    // trigger a main execution
                    let context = self.context.clone();
                    let memstore = self.memory_store.clone();
                    let pending_txns = self.pending_transactions.clone();
                    Self::main_run_inner(
                        memstore,
                        context,
                        pending_txns,
                        pre_exec_res.clone()).await;
                },

                // Merge pre-exec results
                Some(msg) = in_traffic_manager.recv() => {
                   if let RemoraMessage::PreExecResult(tx_res) = msg {
                        pre_exec_res.insert(*tx_res.tx_effects.transaction_digest(), tx_res);
                    }
                }
            }
        }
    }
}
