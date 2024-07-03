// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{random_object_ref, ExecutionDigests, ObjectID, VersionNumber};
use sui_types::committee::Committee;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEventsDigest,
};
use sui_types::effects::{
    TestEffectsBuilder, TransactionEffects, TransactionEffectsAPI, TransactionEvents,
};
use sui_types::error::SuiResult;
use sui_types::event::Event;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber, CheckpointSummary,
    SignedCheckpointSummary,
};
use sui_types::transaction::Transaction;

use sui_storage::key_value_store::*;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

fn random_tx() -> Transaction {
    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas = random_object_ref();
    TestTransactionBuilder::new(sender, gas, 1)
        .transfer(random_object_ref(), sender)
        .build_and_sign(&key)
}

fn random_fx() -> TransactionEffects {
    let tx = random_tx();
    TestEffectsBuilder::new(tx.data()).build()
}

fn random_events() -> TransactionEvents {
    let event = Event::random_for_testing();
    TransactionEvents { data: vec![event] }
}

#[derive(Default)]
struct MockTxStore {
    txs: HashMap<TransactionDigest, Transaction>,
    fxs: HashMap<TransactionDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,
    checkpoint_summaries: HashMap<CheckpointSequenceNumber, CertifiedCheckpointSummary>,
    checkpoint_contents: HashMap<CheckpointSequenceNumber, CheckpointContents>,
    checkpoint_summaries_by_digest: HashMap<CheckpointDigest, CertifiedCheckpointSummary>,
    checkpoint_contents_by_digest: HashMap<CheckpointContentsDigest, CheckpointContents>,
    tx_to_checkpoint: HashMap<TransactionDigest, CheckpointSequenceNumber>,
    objects: HashMap<ObjectKey, Object>,

    next_seq_number: u64,
}

impl MockTxStore {
    fn new() -> Self {
        Self::default()
    }

    fn add_tx(&mut self, tx: Transaction) {
        self.txs.insert(*tx.digest(), tx);
    }

    fn add_fx(&mut self, fx: TransactionEffects) {
        self.fxs.insert(*fx.transaction_digest(), fx);
    }

    fn add_events(&mut self, events: TransactionEvents) {
        self.events.insert(events.digest(), events);
    }

    fn add_random_tx(&mut self) -> Transaction {
        let tx = random_tx();
        self.add_tx(tx.clone());
        tx
    }

    fn add_random_fx(&mut self) -> TransactionEffects {
        let fx = random_fx();
        self.add_fx(fx.clone());
        fx
    }

    fn add_random_events(&mut self) -> TransactionEvents {
        let events = random_events();
        self.add_events(events.clone());
        events
    }

    fn add_random_checkpoint(&mut self) -> (CertifiedCheckpointSummary, CheckpointContents) {
        let contents =
            CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);

        let next_seq = self.next_seq_number;
        self.next_seq_number += 1;

        let (committee, keys) = Committee::new_simple_test_committee_of_size(1);
        let summary = CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            committee.epoch,
            next_seq,
            1,
            &contents,
            None,
            Default::default(),
            None,
            0,
            Vec::new(),
        );

        let signed = SignedCheckpointSummary::new(
            committee.epoch,
            summary.clone(),
            &keys[0],
            keys[0].public().into(),
        );
        let sign_info = signed.into_sig();

        let certified =
            CertifiedCheckpointSummary::new(summary.clone(), vec![sign_info], &committee).unwrap();

        self.checkpoint_summaries
            .insert(summary.sequence_number, certified.clone());
        self.checkpoint_contents
            .insert(summary.sequence_number, contents.clone());
        self.checkpoint_summaries_by_digest
            .insert(*certified.digest(), certified.clone());
        self.checkpoint_contents_by_digest
            .insert(*contents.digest(), contents.clone());
        (certified, contents)
    }
}

impl From<MockTxStore> for TransactionKeyValueStore {
    fn from(store: MockTxStore) -> Self {
        TransactionKeyValueStore::new(
            "mock_tx_store",
            KeyValueStoreMetrics::new_for_tests(),
            Arc::new(store),
        )
    }
}

#[async_trait]
impl TransactionKeyValueStoreTrait for MockTxStore {
    async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
        events: &[TransactionEventsDigest],
    ) -> SuiResult<(
        Vec<Option<Transaction>>,
        Vec<Option<TransactionEffects>>,
        Vec<Option<TransactionEvents>>,
    )> {
        let mut txs = Vec::new();
        for digest in transactions {
            txs.push(self.txs.get(digest).cloned());
        }

        let mut fxs = Vec::new();
        for digest in effects {
            fxs.push(self.fxs.get(digest).cloned());
        }

        let mut evts = Vec::new();
        for digest in events {
            evts.push(self.events.get(digest).cloned());
        }

        Ok((txs, fxs, evts))
    }

    async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
        checkpoint_contents_by_digest: &[CheckpointContentsDigest],
    ) -> SuiResult<(
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
    )> {
        let mut summaries = Vec::new();
        for digest in checkpoint_summaries {
            summaries.push(self.checkpoint_summaries.get(digest).cloned());
        }

        let mut contents = Vec::new();
        for digest in checkpoint_contents {
            contents.push(self.checkpoint_contents.get(digest).cloned());
        }

        let mut summaries_by_digest = Vec::new();
        for digest in checkpoint_summaries_by_digest {
            summaries_by_digest.push(self.checkpoint_summaries_by_digest.get(digest).cloned());
        }

        let mut contents_by_digest = Vec::new();
        for digest in checkpoint_contents_by_digest {
            contents_by_digest.push(self.checkpoint_contents_by_digest.get(digest).cloned());
        }

        Ok((summaries, contents, summaries_by_digest, contents_by_digest))
    }

    async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        Ok(self.tx_to_checkpoint.get(&digest).cloned())
    }

    async fn get_object(
        &self,
        object_id: ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.objects.get(&ObjectKey(object_id, version)).cloned())
    }

    async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        Ok(digests
            .iter()
            .map(|digest| self.tx_to_checkpoint.get(digest).cloned())
            .collect())
    }
}

#[tokio::test]
async fn test_get_tx() {
    let mut store = MockTxStore::new();
    let tx = random_tx();
    store.add_tx(tx.clone());

    let store = TransactionKeyValueStore::from(store);

    let result = store.multi_get_tx(&[*tx.digest()]).now_or_never().unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx)]);

    let result = store
        .multi_get_tx(&[TransactionDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[tokio::test]
async fn test_get_fx() {
    let mut store = MockTxStore::new();
    let fx = random_fx();
    store.add_fx(fx.clone());
    let store = TransactionKeyValueStore::from(store);

    let result = store
        .multi_get_fx_by_tx_digest(&[*fx.transaction_digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(fx)]);

    let result = store
        .multi_get_fx_by_tx_digest(&[TransactionDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[tokio::test]
async fn test_get_events() {
    let mut store = MockTxStore::new();
    let events = random_events();
    store.add_events(events.clone());
    let store = TransactionKeyValueStore::from(store);

    let result = store
        .multi_get_events(&[events.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(events)]);

    let result = store
        .multi_get_events(&[TransactionEventsDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[tokio::test]
async fn test_multi_get() {
    let mut store = MockTxStore::new();
    let txns = vec![store.add_random_tx(), store.add_random_tx()];
    let fxs = vec![
        store.add_random_fx(),
        store.add_random_fx(),
        store.add_random_fx(),
    ];
    let events = vec![store.add_random_events(), store.add_random_events()];

    let store = TransactionKeyValueStore::from(store);

    let result = store
        .multi_get(
            &txns.iter().map(|tx| *tx.digest()).collect::<Vec<_>>(),
            &fxs.iter()
                .map(|fx| *fx.transaction_digest())
                .collect::<Vec<_>>(),
            &events
                .iter()
                .map(|events| events.digest())
                .collect::<Vec<_>>(),
        )
        .now_or_never()
        .unwrap();

    let txns = txns.into_iter().map(Some).collect::<Vec<_>>();
    let fxs = fxs.into_iter().map(Some).collect::<Vec<_>>();
    let events = events.into_iter().map(Some).collect::<Vec<_>>();

    assert_eq!(result.unwrap(), (txns, fxs, events));

    let result = store
        .multi_get_events(&[TransactionEventsDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[tokio::test]
async fn test_checkpoints() {
    let mut store = MockTxStore::new();
    let (s1, _c1) = store.add_random_checkpoint();
    let (s2, c2) = store.add_random_checkpoint();

    let store = TransactionKeyValueStore::from(store);

    let result = store
        .multi_get_checkpoints(
            &[s1.sequence_number, s2.sequence_number],
            &[s1.sequence_number, s2.sequence_number],
            &[*s1.digest(), *s2.digest()],
            &[s1.content_digest, s2.content_digest],
        )
        .now_or_never()
        .unwrap()
        .unwrap();

    let summaries_by_seq = result.0;
    let contents_by_seq = result.1;
    let summaries_by_digest = result.2;
    let contents_by_digest = result.3;

    assert_eq!(summaries_by_seq[0].as_ref().unwrap().data(), s1.data());
    assert_eq!(contents_by_seq[1].as_ref().unwrap(), &c2);
    assert_eq!(summaries_by_digest[0].as_ref().unwrap().data(), s1.data());
    assert_eq!(contents_by_digest[1].as_ref().unwrap(), &c2);
}

#[tokio::test]
async fn test_get_tx_from_fallback() {
    let mut store = MockTxStore::new();
    let tx = store.add_random_tx();
    let fx = store.add_random_fx();
    let store = TransactionKeyValueStore::from(store);

    let mut fallback = MockTxStore::new();
    let fallback_tx = fallback.add_random_tx();
    let fallback_fx = fallback.add_random_fx();
    let fallback = TransactionKeyValueStore::from(fallback);

    let fallback = FallbackTransactionKVStore::new_kv(
        store,
        fallback,
        KeyValueStoreMetrics::new_for_tests(),
        "fallback",
    );

    let result = fallback
        .multi_get_tx(&[*tx.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx.clone())]);

    let result = fallback
        .multi_get_tx(&[*fallback_tx.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(fallback_tx.clone())]);

    let result = fallback
        .multi_get_tx(&[TransactionDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);

    let result = fallback
        .multi_get(
            &[*fallback_tx.digest(), *tx.digest()],
            &[*fx.transaction_digest(), *fallback_fx.transaction_digest()],
            &[],
        )
        .now_or_never()
        .unwrap();
    assert_eq!(
        result.unwrap(),
        (
            vec![Some(fallback_tx), Some(tx)],
            vec![Some(fx), Some(fallback_fx)],
            vec![]
        )
    );
}

#[cfg(msim)]
mod simtests {

    use super::*;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Request, Response, Server,
    };
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use sui_macros::sim_test;
    use sui_simulator::configs::constant_latency_ms;
    use sui_storage::http_key_value_store::*;
    use tracing::info;

    async fn test_server(data: Arc<Mutex<HashMap<String, Vec<u8>>>>) {
        let handle = sui_simulator::runtime::Handle::current();
        let builder = handle.create_node();
        let (startup_sender, mut startup_receiver) = tokio::sync::watch::channel(false);
        let startup_sender = Arc::new(startup_sender);
        let _node = builder
            .ip("10.10.10.10".parse().unwrap())
            .name("server")
            .init(move || {
                info!("Server started");
                let data = data.clone();
                let startup_sender = startup_sender.clone();
                async move {
                    let make_svc = make_service_fn(move |_| {
                        let data = data.clone();
                        async {
                            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                                let data = data.clone();
                                async move {
                                    let path = req.uri().path().to_string();
                                    let key = path.trim_start_matches('/');
                                    let value = data.lock().unwrap().get(key).cloned();
                                    info!("Got request for key: {:?}, value: {:?}", key, value);
                                    match value {
                                        Some(v) => {
                                            Ok::<_, Infallible>(Response::new(Body::from(v)))
                                        }
                                        None => Ok::<_, Infallible>(
                                            Response::builder()
                                                .status(hyper::StatusCode::NOT_FOUND)
                                                .body(Body::empty())
                                                .unwrap(),
                                        ),
                                    }
                                }
                            }))
                        }
                    });

                    let addr = SocketAddr::from(([10, 10, 10, 10], 8080));
                    let server = Server::bind(&addr).serve(make_svc);

                    let graceful = server.with_graceful_shutdown(async {
                        tokio::time::sleep(Duration::from_secs(86400)).await;
                    });

                    tokio::spawn(async {
                        let _ = graceful.await;
                    });

                    startup_sender.send(true).ok();
                }
            })
            .build();
        startup_receiver.changed().await.unwrap();
    }

    #[sim_test(config = "constant_latency_ms(250)")]
    async fn test_multi_fetch() {
        let mut data = HashMap::new();

        let tx = random_tx();
        let random_digest = TransactionDigest::random();
        let fx = random_fx();
        let events = random_events();

        {
            let bytes = bcs::to_bytes(&tx).unwrap();
            assert_eq!(tx, bcs::from_bytes::<Transaction>(&bytes).unwrap());

            let bytes = bcs::to_bytes(&fx).unwrap();
            assert_eq!(fx, bcs::from_bytes::<TransactionEffects>(&bytes).unwrap());

            let bytes = bcs::to_bytes(&events).unwrap();
            assert_eq!(
                events,
                bcs::from_bytes::<TransactionEvents>(&bytes).unwrap()
            );
        }

        data.insert(
            format!("{}/tx", encode_digest(tx.digest())),
            bcs::to_bytes(&tx).unwrap(),
        );
        data.insert(
            format!("{}/fx", encode_digest(fx.transaction_digest())),
            bcs::to_bytes(&fx).unwrap(),
        );
        data.insert(
            format!("{}/ev", encode_digest(&events.digest())),
            bcs::to_bytes(&events).unwrap(),
        );

        // a bogus entry with the wrong digest
        data.insert(
            format!("{}/tx", encode_digest(&random_digest)),
            bcs::to_bytes(&tx).unwrap(),
        );

        let server_data = Arc::new(Mutex::new(data));
        test_server(server_data).await;

        let store = HttpKVStore::new("http://10.10.10.10:8080").unwrap();

        // send one request to warm up the client (and open a connection)
        store.multi_get(&[*tx.digest()], &[], &[]).await.unwrap();

        let start_time = Instant::now();
        let result = store
            .multi_get(
                &[*tx.digest(), *random_tx().digest()],
                &[*fx.transaction_digest()],
                &[events.digest()],
            )
            .await
            .unwrap();

        // verify that the request took approximately one round trip despite fetching 4 items,
        // i.e. test that pipelining or multiplexing is working.
        assert!(start_time.elapsed() < Duration::from_millis(600));

        assert_eq!(
            result,
            (vec![Some(tx), None], vec![Some(fx)], vec![Some(events)])
        );

        let result = store.multi_get(&[random_digest], &[], &[]).await.unwrap();
        assert_eq!(result, (vec![None], vec![], vec![]));
    }
}
