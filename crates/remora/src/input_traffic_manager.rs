use core::panic;
use dashmap::DashMap;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_vm_runtime::move_vm::MoveVM;
use prometheus::proto;
use sui_types::in_memory_storage::InMemoryStorage;
use sui_types::inner_temporary_store::InnerTemporaryStore;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use sui_adapter_latest::programmable_transactions::context;
use sui_adapter_latest::{adapter, execution_engine};
use sui_config::genesis::Genesis;
use sui_core::authority::authority_store_tables::LiveObject;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_move_natives;
use sui_protocol_config::ProtocolConfig;
use sui_single_node_benchmark::benchmark_context::BenchmarkContext;
use sui_single_node_benchmark::mock_storage::InMemoryObjectStore;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::{
    CertifiedTransaction, Transaction, TransactionDataAPI, VerifiedCertificate,
    VerifiedTransaction, DEFAULT_VALIDATOR_GAS_PRICE,
};
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::digests::{ChainIdentifier, ObjectDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::epoch_data::EpochData;
use sui_types::error::SuiError;
use sui_types::execution_mode;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::metrics::LimitsMetrics;
use sui_types::object::Object;
use sui_types::storage::{
    BackingPackageStore, ChildObjectResolver, DeleteKind, GetSharedLocks, ObjectStore, ParentSync,
    WriteKind,
};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemStateTrait};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

use super::types::*;
use crate::tx_gen_agent::{generate_benchmark_ctx_workload, WORKLOAD, COMPONENT};
use crate::tx_gen_agent::generate_benchmark_txs;
use crate::{metrics::Metrics, types::WritableObjectStore};

/*****************************************************************************************
 *                                    Input Traffic Manager in Primary                   *
 *****************************************************************************************/


pub async fn input_traffic_manager_run(
    in_channel: &mut mpsc::Receiver<NetworkMessage>,
    out_consensus: &mpsc::UnboundedSender<RemoraMessage>,
    out_executor: &mpsc::UnboundedSender<RemoraMessage>,
    my_id: u16,
) {
    let mut counter = 0;
    loop {
        tokio::select! {
            Some(msg) = in_channel.recv() => {
                println!("{} receive a msg", my_id);
                let msg = msg.payload;
                if let RemoraMessage::ProposeExec(ref full_tx) = msg {
                    if let Err(e) = out_consensus.send(msg) {
                        eprintln!("Failed to forward to consensus engine: {:?}", e);
                    };
                } else if let RemoraMessage::PreExecResult(ref full_tx) = msg {
                    if let Err(e) = out_executor.send(msg) {
                        eprintln!("Failed to forward to executor engine: {:?}", e);
                    };
                } else {
                    eprintln!("PRI {} received unexpected message from: {:?}", my_id, msg);
                    panic!("unexpected message");
                };
            },
        }
    }
}
