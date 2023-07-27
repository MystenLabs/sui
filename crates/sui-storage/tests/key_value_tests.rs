// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::FutureExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use sui_storage::sharded_lru::ShardedLruCache;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::random_object_ref;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::event::Event;
use sui_types::message_envelope::Message;
use sui_types::transaction::Transaction;

use sui_storage::key_value_store::*;

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
    fxs: HashMap<TransactionEffectsDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,
    log: Arc<Mutex<Vec<Key>>>,
}

impl MockTxStore {
    fn new() -> Self {
        Self {
            txs: HashMap::new(),
            fxs: HashMap::new(),
            events: HashMap::new(),
            log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn add_tx(&mut self, tx: Transaction) {
        self.txs.insert(*tx.digest(), tx);
    }

    fn add_fx(&mut self, fx: TransactionEffects) {
        self.fxs.insert(fx.digest(), fx);
    }

    fn add_events(&mut self, events: TransactionEvents) {
        self.events.insert(events.digest(), events);
    }
}

#[async_trait]
impl TransactionKeyValueStore for MockTxStore {
    async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>> {
        let mut values = Vec::new();
        for key in keys {
            self.log.lock().unwrap().push(*key);
            let value = match key {
                Key::Tx(digest) => self.txs.get(digest).map(|tx| Value::Tx(tx.clone().into())),
                Key::Fx(digest) => self.fxs.get(digest).map(|fx| Value::Fx(fx.clone().into())),
                Key::Events(digest) => self
                    .events
                    .get(digest)
                    .map(|events| Value::Events(events.clone().into())),
                Key::FxByTxDigest(digest) => self
                    .fxs
                    .values()
                    .find(|fx| fx.transaction_digest() == digest)
                    .map(|fx| Value::Fx(fx.clone().into())),
            };
            values.push(value);
        }
        Ok(values)
    }
}

#[test]
fn test_caching_kv_store() {
    let mut store = MockTxStore::new();
    let log = store.log.clone();

    let tx1 = random_tx();
    let tx2 = random_tx();
    store.add_tx(tx1.clone());
    store.add_tx(tx2.clone());

    let cache = ShardedLruCache::new(100, 2);
    let store = CachingKVStore::new(Box::new(store), cache);

    let result = store.multi_get_tx(&[*tx1.digest()]).now_or_never().unwrap();

    assert_eq!(result.unwrap(), vec![Some(tx1.clone())]);
    // First request goes to the MockTxStore
    assert_eq!(log.lock().unwrap().clone(), vec![Key::Tx(*tx1.digest())]);

    let result = store.multi_get_tx(&[*tx1.digest()]).now_or_never().unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx1.clone())]);

    // Second request is satisfied by the cache
    assert_eq!(log.lock().unwrap().clone(), vec![Key::Tx(*tx1.digest())]);

    // mix of cached and uncached keys works
    let result = store
        .multi_get_tx(&[*tx1.digest(), *tx2.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx1.clone()), Some(tx2.clone())]);
    // request for tx2 goes to the MockTxStore, but tx1 is cached
    assert_eq!(
        log.lock().unwrap().clone(),
        vec![Key::Tx(*tx1.digest()), Key::Tx(*tx2.digest())]
    );
}

#[test]
fn test_get_tx() {
    let mut store = MockTxStore::new();
    let tx = random_tx();
    store.add_tx(tx.clone());

    let result = store.multi_get_tx(&[*tx.digest()]).now_or_never().unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx)]);

    let result = store
        .multi_get_tx(&[TransactionDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[test]
fn test_get_fx() {
    let mut store = MockTxStore::new();
    let fx = random_fx();
    store.add_fx(fx.clone());

    let result = store.multi_get_fx(&[fx.digest()]).now_or_never().unwrap();
    assert_eq!(result.unwrap(), vec![Some(fx)]);

    let result = store
        .multi_get_fx(&[TransactionEffectsDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[test]
fn test_get_events() {
    let mut store = MockTxStore::new();
    let events = random_events();
    store.add_events(events.clone());

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

#[test]
fn test_get_tx_from_fallback() {
    let mut store = MockTxStore::new();
    let tx = random_tx();
    store.add_tx(tx.clone());

    let mut fallback = MockTxStore::new();
    let fallback_tx = random_tx();
    fallback.add_tx(fallback_tx.clone());

    let fallback = FallbackTransactionKVStore::new(Box::new(store), Box::new(fallback));

    let result = fallback
        .multi_get_tx(&[*tx.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(tx)]);

    let result = fallback
        .multi_get_tx(&[*fallback_tx.digest()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![Some(fallback_tx)]);

    let result = fallback
        .multi_get_tx(&[TransactionDigest::random()])
        .now_or_never()
        .unwrap();
    assert_eq!(result.unwrap(), vec![None]);
}

#[cfg(msim)]
mod simtests {

    use super::*;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Request, Response, Server, Uri,
    };
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
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
            format!("{}/events", encode_digest(&events.digest())),
            bcs::to_bytes(&events).unwrap(),
        );

        let server_data = Arc::new(Mutex::new(data));
        test_server(server_data).await;

        let keys = vec![
            Key::Tx(*tx.digest()),
            Key::FxByTxDigest(*fx.transaction_digest()),
            Key::Events(events.digest()),
            Key::Tx(*random_tx().digest()),
        ];

        let store = HttpKVStore::new(Uri::from_str("http://10.10.10.10:8080").unwrap()).unwrap();

        // send one request to warm up the client (and open a connection)
        store.multi_get(&[keys[0]]).await.unwrap();

        let start_time = Instant::now();
        let result = store.multi_get(&keys).await.unwrap();
        // verify that the request took approximately one round trip despite fetching 4 items,
        // i.e. test that pipelining or multiplexing is working.
        assert!(start_time.elapsed() < Duration::from_millis(600));

        assert_eq!(result[0].as_ref().unwrap(), &Value::from(tx));
        assert_eq!(result[1].as_ref().unwrap(), &Value::from(fx));
        assert_eq!(result[2].as_ref().unwrap(), &Value::from(events));
        assert!(result[3].is_none());
    }
}
