// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::random_object_ref;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::digests::{TransactionDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::event::Event;
use sui_types::transaction::Transaction;

use sui_storage::key_value_store::*;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;

fn random_tx() -> Transaction {
    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas = random_object_ref();
    TestTransactionBuilder::new(sender, gas, 1)
        .transfer(random_object_ref(), sender)
        .build_and_sign(&key)
}

fn random_fx() -> TransactionEffects {
    let tx = random_tx();
    TransactionEffects::new_with_tx(&tx)
}

fn random_events() -> TransactionEvents {
    let event = Event::random_for_testing();
    TransactionEvents { data: vec![event] }
}

struct MockTxStore {
    txs: HashMap<TransactionDigest, Transaction>,
    fxs: HashMap<TransactionDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,
}

impl MockTxStore {
    fn new() -> Self {
        Self {
            txs: HashMap::new(),
            fxs: HashMap::new(),
            events: HashMap::new(),
        }
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
