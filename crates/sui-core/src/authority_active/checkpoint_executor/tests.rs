// use super::*;
// use crate::authority::AuthorityStore;
// use async_trait::async_trait;
// use fastcrypto::traits::KeyPair;
// use sui_types::{committee::Committee, crypto::AuthorityKeyPair};
// use tempfile::tempdir;
// use tokio::sync::mpsc;

// use std::{sync::Arc, time::Duration};

// use futures::stream::FuturesOrdered;
// use sui_metrics::spawn_monitored_task;
// use sui_types::{
//     base_types::{AuthorityName, ExecutionDigests, TransactionDigest},
//     committee::StakeUnit,
//     error::{SuiError, SuiResult},
//     messages::{TransactionEffects, VerifiedCertificate},
//     messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
// };
// use tokio::{
//     sync::broadcast::{self, error::RecvError},
//     task::JoinHandle,
// };
// use tokio_stream::StreamExt;
// use tracing::{error, info, warn};
// use typed_store::rocks::TypedStoreError;

// use crate::{
//     authority::AuthorityState,
//     checkpoints::{CheckpointMetrics, CheckpointStore},
// };

// #[tokio::test]
// pub async fn checkpoint_executor_test() {
//     let buffer_size = num_cpus::get() * TASKS_PER_CORE * 2;
//     let (executor, state_sync_handle, checkpoint_sender) = init_executor_test(buffer_size);

//     // push new Checkpoints to recv buffer for execution
// }

// async fn init_executor_test(
//     buffer_size: usize,
// ) -> (
//     CheckpointExecutor,
//     sui_network::state_sync::Handle,
//     Sender<VerifiedCheckpoint>,
// ) {
//     let tempdir = tempdir().unwrap();
//     let (keypair, committee) = committee();
//     let (tx_reconfigure_consensus, _rx_reconfigure_consensus) = mpsc::channel(10);
//     let state = Arc::new(
//         AuthorityState::new_for_testing(
//             committee.clone(),
//             &keypair,
//             None,
//             None,
//             tx_reconfigure_consensus,
//         )
//         .await,
//     );

//     let store = CheckpointStore::new(tempdir.path());
//     let metrics = CheckpointMetrics::new_for_tests();
//     let (checkpoint_sender, _) = broadcast::channel(buffer_size);
//     let (sender, _) = mpsc::channel(buffer_size);
//     let state_sync_handle = sui_network::state_sync::Handle {
//         sender,
//         checkpoint_event_sender: checkpoint_sender,
//     };
//     let executor = CheckpointExecutor::new(&state_sync_handle, store, state, metrics).unwrap();

//     (executor, state_sync_handle, checkpoint_sender)
// }

// fn committee() -> (AuthorityKeyPair, Committee) {
//     use std::collections::BTreeMap;
//     use sui_types::crypto::get_key_pair;
//     use sui_types::crypto::AuthorityPublicKeyBytes;

//     let (_authority_address, authority_key): (_, AuthorityKeyPair) = get_key_pair();
//     let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
//     authorities.insert(
//         /* address */ authority_key.public().into(),
//         /* voting right */ 1,
//     );
//     (authority_key, Committee::new(0, authorities).unwrap())
// }

// #[async_trait]
// impl EffectsNotifyRead for Arc<AuthorityStore> {
//     async fn notify_read(
//         &self,
//         digests: Vec<TransactionDigest>,
//     ) -> SuiResult<Vec<TransactionEffects>> {
//         Ok(digests
//             .into_iter()
//             .map(|d| self.get(d.as_ref()).expect("effects not found").clone())
//             .collect())
//     }

//     fn get_effects(
//         &self,
//         digests: &[TransactionDigest],
//     ) -> SuiResult<Vec<Option<TransactionEffects>>> {
//         Ok(digests
//             .iter()
//             .map(|d| self.get(d.as_ref()).cloned())
//             .collect())
//     }
// }
