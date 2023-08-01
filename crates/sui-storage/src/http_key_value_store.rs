// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use hyper::client::HttpConnector;
use hyper::Client;
use hyper::Uri;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use sui_types::{
    digests::{TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, SuiResult},
    transaction::Transaction,
};
use tap::TapFallible;
use tracing::{error, info, trace, warn};
use url::Url;

use crate::key_value_store::{TransactionKeyValueStore, TransactionKeyValueStoreTrait};
use crate::key_value_store_metrics::KeyValueStoreMetrics;

pub struct HttpKVStore {
    base_url: Url,
    client: Arc<Client<HttpsConnector<HttpConnector>>>,
}

pub fn encode_digest<T: AsRef<[u8]>>(digest: &T) -> String {
    base64_url::encode(digest)
}

trait IntoSuiResult<T> {
    fn into_sui_result(self) -> SuiResult<T>;
}

impl<T, E> IntoSuiResult<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn into_sui_result(self) -> SuiResult<T> {
        self.map_err(|e| SuiError::GenericStorageError(e.to_string()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tx(TransactionDigest),
    Fx(TransactionDigest),
    Events(TransactionEventsDigest),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Tx(Box<Transaction>),
    Fx(Box<TransactionEffects>),
    Events(Box<TransactionEvents>),
}

fn key_to_path_elements(key: &Key) -> SuiResult<(String, &'static str)> {
    match key {
        Key::Tx(digest) => Ok((encode_digest(digest), "tx")),
        Key::Fx(digest) => Ok((encode_digest(digest), "fx")),
        Key::Events(digest) => Ok((encode_digest(digest), "ev")),
    }
}

impl HttpKVStore {
    pub fn new_kv(
        base_url: &str,
        metrics: Arc<KeyValueStoreMetrics>,
    ) -> SuiResult<TransactionKeyValueStore> {
        let inner = Arc::new(Self::new(base_url)?);
        Ok(TransactionKeyValueStore::new("http", metrics, inner))
    }

    pub fn new(base_url: &str) -> SuiResult<Self> {
        info!("creating HttpKVStore with base_url: {}", base_url);
        let http = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http2()
            .build();

        let client = Client::builder()
            .http2_only(true)
            .build::<_, hyper::Body>(http);

        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{}/", base_url)
        };

        let base_url = Url::parse(&base_url).into_sui_result()?;

        Ok(Self {
            base_url,
            client: Arc::new(client),
        })
    }

    fn get_url(&self, key: &Key) -> SuiResult<Uri> {
        let (digest, item_type) = key_to_path_elements(key)?;
        let joined = self
            .base_url
            .join(&format!("{}/{}", digest, item_type))
            .into_sui_result()?;
        Uri::from_str(joined.as_str()).into_sui_result()
    }

    async fn multi_fetch(&self, uris: Vec<Key>) -> Vec<SuiResult<Option<Bytes>>> {
        let uris_vec = uris.to_vec();
        let fetches = stream::iter(
            uris_vec
                .into_iter()
                .enumerate()
                .map(|(_i, uri)| self.fetch(uri)),
        );
        fetches.buffered(uris.len()).collect::<Vec<_>>().await
    }

    async fn fetch(&self, key: Key) -> SuiResult<Option<Bytes>> {
        let uri = self.get_url(&key)?;
        trace!("fetching uri: {}", uri);
        let resp = self.client.get(uri.clone()).await.into_sui_result()?;
        trace!(
            "got response {} for uri: {}, len: {}",
            uri,
            resp.status(),
            resp.headers().len()
        );
        // return None if 400
        if resp.status().is_success() {
            hyper::body::to_bytes(resp.into_body())
                .await
                .map(Some)
                .into_sui_result()
        } else {
            Ok(None)
        }
    }
}

fn deser<K, T>(key: &K, bytes: &[u8]) -> Option<T>
where
    K: std::fmt::Debug,
    T: for<'de> Deserialize<'de>,
{
    bcs::from_bytes(bytes)
        .tap_err(|e| warn!("Error deserializing data for key {:?}: {:?}", key, e))
        .ok()
}

#[async_trait]
impl TransactionKeyValueStoreTrait for HttpKVStore {
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
        let num_txns = transactions.len();
        let num_effects = effects.len();
        let num_events = events.len();

        let keys = transactions
            .iter()
            .map(|tx| Key::Tx(*tx))
            .chain(effects.iter().map(|fx| Key::Fx(*fx)))
            .chain(events.iter().map(|events| Key::Events(*events)))
            .collect::<Vec<_>>();

        let fetches = self.multi_fetch(keys).await;
        let txn_slice = fetches[..num_txns].to_vec();
        let fx_slice = fetches[num_txns..num_txns + num_effects].to_vec();
        let events_slice = fetches[num_txns + num_effects..].to_vec();

        fn map_fetch<'a, Digest>(
            fetch: (&'a SuiResult<Option<Bytes>>, &'a Digest),
        ) -> Option<(&'a Bytes, &'a Digest)>
        where
            Digest: std::fmt::Debug,
        {
            let (fetch, digest) = fetch;
            match fetch {
                Ok(Some(bytes)) => Some((bytes, digest)),
                Ok(None) => None,
                Err(err) => {
                    warn!("Error fetching key: {:?}, error: {:?}", digest, err);
                    None
                }
            }
        }

        fn deser_check_digest<T, D: std::fmt::Debug>(
            digest: &D,
            bytes: &Bytes,
            get_expected_digest: impl FnOnce(&T) -> D,
        ) -> Option<T>
        where
            D: std::fmt::Debug + PartialEq,
            T: for<'de> Deserialize<'de>,
        {
            deser(digest, bytes).and_then(|o: T| {
                let expected_digest = get_expected_digest(&o);
                if expected_digest == *digest {
                    Some(o)
                } else {
                    error!(
                        "Digest mismatch - expected: {:?}, got: {:?}",
                        digest, expected_digest,
                    );
                    None
                }
            })
        }

        let txn_results = txn_slice
            .iter()
            .take(num_txns)
            .zip(transactions.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, digest)| {
                    deser_check_digest(digest, bytes, |tx: &Transaction| *tx.digest())
                })
            })
            .collect::<Vec<_>>();

        let fx_results = fx_slice
            .iter()
            .take(num_effects)
            .zip(effects.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, digest)| {
                    deser_check_digest(digest, bytes, |fx: &TransactionEffects| {
                        *fx.transaction_digest()
                    })
                })
            })
            .collect::<Vec<_>>();

        let events_results = events_slice
            .iter()
            .take(num_events)
            .zip(events.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, digest)| {
                    deser_check_digest(digest, bytes, |events: &TransactionEvents| events.digest())
                })
            })
            .collect::<Vec<_>>();

        Ok((txn_results, fx_results, events_results))
    }
}
