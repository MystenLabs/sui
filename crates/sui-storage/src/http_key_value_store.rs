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
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, SuiResult},
    message_envelope::Message,
    transaction::Transaction,
};
use tap::TapFallible;
use tracing::{error, info, trace, warn};
use url::Url;

use crate::key_value_store::{Key, TransactionKeyValueStore, Value};

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

fn key_to_path_elements(key: &Key) -> SuiResult<(String, &'static str)> {
    match key {
        Key::Tx(digest) => Ok((encode_digest(digest), "tx")),
        Key::Fx(digest) => Err(SuiError::UnsupportedFeatureError {
            error: format!(
                "fetching fx by fx digest not supported (digest: {:?})",
                digest
            ),
        }),
        Key::Events(digest) => Ok((encode_digest(digest), "events")),
        Key::FxByTxDigest(digest) => Ok((encode_digest(digest), "fx")),
    }
}

impl HttpKVStore {
    pub fn new(base_url: Uri) -> SuiResult<Self> {
        info!("creating HttpKVStore with base_url: {}", base_url);
        let http = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http2()
            .build();

        let client = Client::builder()
            .http2_only(true)
            .build::<_, hyper::Body>(http);
        let base_url = Url::parse(&base_url.to_string()).into_sui_result()?;

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

    async fn multi_fetch(&self, uris: &[Key]) -> Vec<SuiResult<Bytes>> {
        let uris_vec = uris.to_vec();
        let fetches = stream::iter(
            uris_vec
                .into_iter()
                .enumerate()
                .map(|(_i, uri)| self.fetch(uri)),
        );
        fetches.buffered(uris.len()).collect::<Vec<_>>().await
    }

    async fn fetch(&self, key: Key) -> SuiResult<Bytes> {
        let uri = self.get_url(&key)?;
        trace!("fetching uri: {}", uri);
        let resp = self.client.get(uri.clone()).await.into_sui_result()?;
        trace!(
            "got response for uri: {}, len: {}",
            uri,
            resp.headers().len()
        );
        hyper::body::to_bytes(resp.into_body())
            .await
            .into_sui_result()
    }
}

fn deser<T>(key: &Key, bytes: &[u8]) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
{
    bcs::from_bytes(bytes)
        .tap_err(|e| warn!("Error deserializing data for key {:?}: {:?}", key, e))
        .ok()
}

#[async_trait]
impl TransactionKeyValueStore for HttpKVStore {
    async fn multi_get(&self, keys: &[Key]) -> SuiResult<Vec<Option<Value>>> {
        let fetches = self.multi_fetch(keys).await;

        let values: Vec<_> = fetches
            .into_iter()
            .zip(keys.iter())
            .map(|(fetch, key)| match fetch {
                Ok(bytes) => Some((bytes, key)),
                Err(err) => {
                    warn!("Error fetching key: {:?}, error: {:?}", key, err);
                    None
                }
            })
            .map(|maybe_bytes| match maybe_bytes {
                Some((bytes, key)) => match key {
                    Key::Tx(digest) => {
                        let tx = deser(key, &bytes).and_then(|tx: Transaction| {
                            if tx.digest() == digest {
                                Some(tx)
                            } else {
                                error!(
                                    "Digest mismatch for tx, expected: {:?}, got: {:?}",
                                    digest,
                                    tx.digest()
                                );
                                None
                            }
                        })?;
                        Some(Value::Tx(Box::new(tx)))
                    }
                    Key::Fx(digest) => {
                        let fx = deser(key, &bytes).and_then(|fx: TransactionEffects| {
                            if fx.digest() == *digest {
                                Some(fx)
                            } else {
                                error!(
                                    "Digest mismatch for fx, expected: {:?}, got: {:?}",
                                    digest,
                                    fx.digest()
                                );
                                None
                            }
                        })?;
                        Some(Value::Fx(Box::new(fx)))
                    }
                    Key::Events(digest) => {
                        let events = deser(key, &bytes).and_then(|events: TransactionEvents| {
                            if events.digest() == *digest {
                                Some(events)
                            } else {
                                error!(
                                    "Digest mismatch for events, expected: {:?}, got: {:?}",
                                    digest,
                                    events.digest()
                                );
                                None
                            }
                        })?;
                        Some(Value::Events(Box::new(events)))
                    }
                    Key::FxByTxDigest(digest) => {
                        let fx = deser(key, &bytes).and_then(|fx: TransactionEffects| {
                            let tx_digest = fx.transaction_digest();
                            if tx_digest == digest {
                                Some(fx)
                            } else {
                                error!(
                                    "expected TransactionEffects for tx: {:?}, got: {:?}",
                                    digest, tx_digest
                                );
                                None
                            }
                        })?;
                        Some(Value::Fx(Box::new(fx)))
                    }
                },
                None => None,
            })
            .collect();

        Ok(values)
    }
}
