// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use moka::sync::{Cache as MokaCache, CacheBuilder as MokaCacheBuilder};
use reqwest::header::{HeaderValue, CONTENT_LENGTH};
use reqwest::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::{
    digests::{CheckpointContentsDigest, CheckpointDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, SuiResult},
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
    },
    transaction::Transaction,
};
use tap::{TapFallible, TapOptional};
use tracing::{error, info, instrument, trace, warn};

use crate::key_value_store::{TransactionKeyValueStore, TransactionKeyValueStoreTrait};
use crate::key_value_store_metrics::KeyValueStoreMetrics;

pub struct HttpKVStore {
    base_url: Url,
    client: Client,
    cache: MokaCache<Url, Bytes>,
    metrics: Arc<KeyValueStoreMetrics>,
}

pub fn encode_digest<T: AsRef<[u8]>>(digest: &T) -> String {
    base64_url::encode(digest)
}

// for non-digest keys, we need a tag to make sure we don't have collisions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaggedKey {
    CheckpointSequenceNumber(CheckpointSequenceNumber),
}

pub fn encoded_tagged_key(key: &TaggedKey) -> String {
    let bytes = bcs::to_bytes(key).expect("failed to serialize key");
    base64_url::encode(&bytes)
}

pub fn encode_object_key(object_id: &ObjectID, version: &VersionNumber) -> String {
    let bytes =
        bcs::to_bytes(&ObjectKey(*object_id, *version)).expect("failed to serialize object key");
    base64_url::encode(&bytes)
}

trait IntoSuiResult<T> {
    fn into_sui_result(self) -> SuiResult<T>;
}

impl<T, E> IntoSuiResult<T> for Result<T, E>
where
    E: std::error::Error,
{
    fn into_sui_result(self) -> SuiResult<T> {
        self.map_err(|e| SuiError::Storage(e.to_string()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tx(TransactionDigest),
    Fx(TransactionDigest),
    CheckpointContents(CheckpointSequenceNumber),
    CheckpointSummary(CheckpointSequenceNumber),
    CheckpointContentsByDigest(CheckpointContentsDigest),
    CheckpointSummaryByDigest(CheckpointDigest),
    TxToCheckpoint(TransactionDigest),
    ObjectKey(ObjectID, VersionNumber),
    EventsByTxDigest(TransactionDigest),
}

impl Key {
    /// Return a string representation of the key type
    pub fn ty(&self) -> &'static str {
        match self {
            Key::Tx(_) => "tx",
            Key::Fx(_) => "fx",
            Key::CheckpointContents(_) => "cc",
            Key::CheckpointSummary(_) => "cs",
            Key::CheckpointContentsByDigest(_) => "cc",
            Key::CheckpointSummaryByDigest(_) => "cs",
            Key::TxToCheckpoint(_) => "tx2c",
            Key::ObjectKey(_, _) => "ob",
            Key::EventsByTxDigest(_) => "evtx",
        }
    }

    pub fn encode(&self) -> String {
        match self {
            Key::Tx(digest) => encode_digest(digest),
            Key::Fx(digest) => encode_digest(digest),
            Key::CheckpointContents(seq) => {
                encoded_tagged_key(&TaggedKey::CheckpointSequenceNumber(*seq))
            }
            Key::CheckpointSummary(seq) => {
                encoded_tagged_key(&TaggedKey::CheckpointSequenceNumber(*seq))
            }
            Key::CheckpointContentsByDigest(digest) => encode_digest(digest),
            Key::CheckpointSummaryByDigest(digest) => encode_digest(digest),
            Key::TxToCheckpoint(digest) => encode_digest(digest),
            Key::ObjectKey(object_id, version) => encode_object_key(object_id, version),
            Key::EventsByTxDigest(digest) => encode_digest(digest),
        }
    }

    pub fn to_path_elements(&self) -> (String, &'static str) {
        (self.encode(), self.ty())
    }
}

#[derive(Clone, Debug)]
enum Value {
    Tx(Box<Transaction>),
    Fx(Box<TransactionEffects>),
    Events(Box<TransactionEvents>),
    CheckpointContents(Box<CheckpointContents>),
    CheckpointSummary(Box<CertifiedCheckpointSummary>),
    TxToCheckpoint(CheckpointSequenceNumber),
}

pub fn path_elements_to_key(digest: &str, type_: &str) -> anyhow::Result<Key> {
    let decoded_digest = base64_url::decode(digest)?;

    match type_ {
        "tx" => Ok(Key::Tx(TransactionDigest::try_from(decoded_digest)?)),
        "fx" => Ok(Key::Fx(TransactionDigest::try_from(decoded_digest)?)),
        "cc" => {
            // first try to decode as digest, otherwise try to decode as tagged key
            match CheckpointContentsDigest::try_from(decoded_digest.clone()) {
                Err(_) => {
                    let tagged_key = bcs::from_bytes(&decoded_digest)?;
                    match tagged_key {
                        TaggedKey::CheckpointSequenceNumber(seq) => {
                            Ok(Key::CheckpointContents(seq))
                        }
                    }
                }
                Ok(cc_digest) => Ok(Key::CheckpointContentsByDigest(cc_digest)),
            }
        }
        "cs" => {
            // first try to decode as digest, otherwise try to decode as tagged key
            match CheckpointDigest::try_from(decoded_digest.clone()) {
                Err(_) => {
                    let tagged_key = bcs::from_bytes(&decoded_digest)?;
                    match tagged_key {
                        TaggedKey::CheckpointSequenceNumber(seq) => Ok(Key::CheckpointSummary(seq)),
                    }
                }
                Ok(cs_digest) => Ok(Key::CheckpointSummaryByDigest(cs_digest)),
            }
        }
        "tx2c" => Ok(Key::TxToCheckpoint(TransactionDigest::try_from(
            decoded_digest,
        )?)),
        "ob" => {
            let object_key: ObjectKey = bcs::from_bytes(&decoded_digest)?;
            Ok(Key::ObjectKey(object_key.0, object_key.1))
        }
        _ => Err(anyhow::anyhow!("Invalid type: {}", type_)),
    }
}

impl HttpKVStore {
    pub fn new_kv(
        base_url: &str,
        cache_size: u64,
        metrics: Arc<KeyValueStoreMetrics>,
    ) -> SuiResult<TransactionKeyValueStore> {
        let inner = Arc::new(Self::new(base_url, cache_size, metrics.clone())?);
        Ok(TransactionKeyValueStore::new("http", metrics, inner))
    }

    pub fn new(
        base_url: &str,
        cache_size: u64,
        metrics: Arc<KeyValueStoreMetrics>,
    ) -> SuiResult<Self> {
        info!("creating HttpKVStore with base_url: {}", base_url);

        let client = Client::builder().http2_prior_knowledge().build().unwrap();

        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{}/", base_url)
        };

        let base_url = Url::parse(&base_url).into_sui_result()?;

        let cache = MokaCacheBuilder::new(cache_size)
            .time_to_idle(Duration::from_secs(600))
            .build();

        Ok(Self {
            base_url,
            client,
            cache,
            metrics,
        })
    }

    fn get_url(&self, key: &Key) -> SuiResult<Url> {
        let (digest, item_type) = key.to_path_elements();
        let joined = self
            .base_url
            .join(&format!("{}/{}", digest, item_type))
            .into_sui_result()?;
        Url::from_str(joined.as_str()).into_sui_result()
    }

    async fn multi_fetch(&self, uris: Vec<Key>) -> Vec<SuiResult<Option<Bytes>>> {
        let uris_vec = uris.to_vec();
        let fetches = stream::iter(uris_vec.into_iter().map(|url| self.fetch(url)));
        fetches.buffered(uris.len()).collect::<Vec<_>>().await
    }

    async fn fetch(&self, key: Key) -> SuiResult<Option<Bytes>> {
        let url = self.get_url(&key)?;

        trace!("fetching url: {}", url);

        if let Some(res) = self.cache.get(&url) {
            trace!("found cached data for url: {}, len: {:?}", url, res.len());
            self.metrics
                .key_value_store_num_fetches_success
                .with_label_values(&["http_cache", key.ty()])
                .inc();
            return Ok(Some(res));
        }

        self.metrics
            .key_value_store_num_fetches_not_found
            .with_label_values(&["http_cache", key.ty()])
            .inc();

        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .into_sui_result()?;
        trace!(
            "got response {} for url: {}, len: {:?}",
            url,
            resp.status(),
            resp.headers()
                .get(CONTENT_LENGTH)
                .unwrap_or(&HeaderValue::from_static("0"))
        );
        // return None if 400
        if resp.status().is_success() {
            let bytes = resp.bytes().await.into_sui_result()?;
            self.cache.insert(url, bytes.clone());

            Ok(Some(bytes))
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

fn map_fetch<'a, K>(fetch: (&'a SuiResult<Option<Bytes>>, &'a K)) -> Option<(&'a Bytes, &'a K)>
where
    K: std::fmt::Debug,
{
    let (fetch, key) = fetch;
    match fetch {
        Ok(Some(bytes)) => Some((bytes, key)),
        Ok(None) => None,
        Err(err) => {
            warn!("Error fetching key: {:?}, error: {:?}", key, err);
            None
        }
    }
}

fn multi_split_slice<'a, T>(slice: &'a [T], lengths: &'a [usize]) -> Vec<&'a [T]> {
    let mut start = 0;
    lengths
        .iter()
        .map(|length| {
            let end = start + length;
            let result = &slice[start..end];
            start = end;
            result
        })
        .collect()
}

fn deser_check_digest<T, D>(
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

#[async_trait]
impl TransactionKeyValueStoreTrait for HttpKVStore {
    #[instrument(level = "trace", skip_all)]
    async fn multi_get(
        &self,
        transactions: &[TransactionDigest],
        effects: &[TransactionDigest],
    ) -> SuiResult<(Vec<Option<Transaction>>, Vec<Option<TransactionEffects>>)> {
        let num_txns = transactions.len();
        let num_effects = effects.len();

        let keys = transactions
            .iter()
            .map(|tx| Key::Tx(*tx))
            .chain(effects.iter().map(|fx| Key::Fx(*fx)))
            .collect::<Vec<_>>();

        let fetches = self.multi_fetch(keys).await;
        let txn_slice = fetches[..num_txns].to_vec();
        let fx_slice = fetches[num_txns..num_txns + num_effects].to_vec();

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
        Ok((txn_results, fx_results))
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_checkpoints(
        &self,
        checkpoint_summaries: &[CheckpointSequenceNumber],
        checkpoint_contents: &[CheckpointSequenceNumber],
        checkpoint_summaries_by_digest: &[CheckpointDigest],
    ) -> SuiResult<(
        Vec<Option<CertifiedCheckpointSummary>>,
        Vec<Option<CheckpointContents>>,
        Vec<Option<CertifiedCheckpointSummary>>,
    )> {
        let keys = checkpoint_summaries
            .iter()
            .map(|cp| Key::CheckpointSummary(*cp))
            .chain(
                checkpoint_contents
                    .iter()
                    .map(|cp| Key::CheckpointContents(*cp)),
            )
            .chain(
                checkpoint_summaries_by_digest
                    .iter()
                    .map(|cp| Key::CheckpointSummaryByDigest(*cp)),
            )
            .collect::<Vec<_>>();

        let summaries_len = checkpoint_summaries.len();
        let contents_len = checkpoint_contents.len();
        let summaries_by_digest_len = checkpoint_summaries_by_digest.len();

        let fetches = self.multi_fetch(keys).await;

        let input_slices = [summaries_len, contents_len, summaries_by_digest_len];

        let result_slices = multi_split_slice(&fetches, &input_slices);

        let summaries_results = result_slices[0]
            .iter()
            .zip(checkpoint_summaries.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes
                    .and_then(|(bytes, seq)| deser::<_, CertifiedCheckpointSummary>(seq, bytes))
            })
            .collect::<Vec<_>>();

        let contents_results = result_slices[1]
            .iter()
            .zip(checkpoint_contents.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, seq)| deser::<_, CheckpointContents>(seq, bytes))
            })
            .collect::<Vec<_>>();

        let summaries_by_digest_results = result_slices[2]
            .iter()
            .zip(checkpoint_summaries_by_digest.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, digest)| {
                    deser_check_digest(digest, bytes, |s: &CertifiedCheckpointSummary| *s.digest())
                })
            })
            .collect::<Vec<_>>();
        Ok((
            summaries_results,
            contents_results,
            summaries_by_digest_results,
        ))
    }

    #[instrument(level = "trace", skip_all)]
    async fn deprecated_get_transaction_checkpoint(
        &self,
        digest: TransactionDigest,
    ) -> SuiResult<Option<CheckpointSequenceNumber>> {
        let key = Key::TxToCheckpoint(digest);
        self.fetch(key).await.map(|maybe| {
            maybe.and_then(|bytes| deser::<_, CheckpointSequenceNumber>(&key, bytes.as_ref()))
        })
    }

    #[instrument(level = "trace", skip_all)]
    async fn get_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let key = Key::ObjectKey(object_id, version);
        self.fetch(key).await.map(|maybe| {
            maybe
                .and_then(|bytes| deser::<_, Object>(&key, bytes.as_ref()))
                .tap_some(|_| {
                    self.metrics
                        .key_value_store_num_fetches_success
                        .with_label_values(&["http", key.ty()])
                        .inc();
                })
                .tap_none(|| {
                    self.metrics
                        .key_value_store_num_fetches_not_found
                        .with_label_values(&["http", key.ty()])
                        .inc();
                })
        })
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<CheckpointSequenceNumber>>> {
        let keys = digests
            .iter()
            .map(|digest| Key::TxToCheckpoint(*digest))
            .collect::<Vec<_>>();

        let fetches = self.multi_fetch(keys).await;

        let results = fetches
            .iter()
            .zip(digests.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes
                    .and_then(|(bytes, key)| deser::<_, CheckpointSequenceNumber>(&key, bytes))
            })
            .collect::<Vec<_>>();

        Ok(results)
    }

    #[instrument(level = "trace", skip_all)]
    async fn multi_get_events_by_tx_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        let keys = digests
            .iter()
            .map(|digest| Key::EventsByTxDigest(*digest))
            .collect::<Vec<_>>();
        Ok(self
            .multi_fetch(keys)
            .await
            .iter()
            .zip(digests.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes
                    .and_then(|(bytes, key)| deser::<_, TransactionEvents>(&key, &bytes.slice(1..)))
            })
            .collect::<Vec<_>>())
    }
}
