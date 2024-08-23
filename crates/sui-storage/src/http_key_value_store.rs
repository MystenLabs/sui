// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt};
use reqwest::header::{HeaderValue, CONTENT_LENGTH};
use reqwest::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use sui_types::{
    digests::{
        CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEventsDigest,
    },
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, SuiResult},
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
    },
    transaction::Transaction,
};
use tap::TapFallible;
use tracing::{error, info, instrument, trace, warn};

use crate::key_value_store::{TransactionKeyValueStore, TransactionKeyValueStoreTrait};
use crate::key_value_store_metrics::KeyValueStoreMetrics;

pub struct HttpKVStore {
    base_url: Url,
    client: Client,
}

pub fn encode_digest<T: AsRef<[u8]>>(digest: &T) -> String {
    base64_url::encode(digest)
}

// for non-digest keys, we need a tag to make sure we don't have collisions
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
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
    Events(TransactionEventsDigest),
    CheckpointContents(CheckpointSequenceNumber),
    CheckpointSummary(CheckpointSequenceNumber),
    CheckpointContentsByDigest(CheckpointContentsDigest),
    CheckpointSummaryByDigest(CheckpointDigest),
    TxToCheckpoint(TransactionDigest),
    ObjectKey(ObjectID, VersionNumber),
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

fn key_to_path_elements(key: &Key) -> SuiResult<(String, &'static str)> {
    match key {
        Key::Tx(digest) => Ok((encode_digest(digest), "tx")),
        Key::Fx(digest) => Ok((encode_digest(digest), "fx")),
        Key::Events(digest) => Ok((encode_digest(digest), "ev")),
        Key::CheckpointContents(seq) => Ok((
            encoded_tagged_key(&TaggedKey::CheckpointSequenceNumber(*seq)),
            "cc",
        )),
        Key::CheckpointSummary(seq) => Ok((
            encoded_tagged_key(&TaggedKey::CheckpointSequenceNumber(*seq)),
            "cs",
        )),
        Key::CheckpointContentsByDigest(digest) => Ok((encode_digest(digest), "cc")),
        Key::CheckpointSummaryByDigest(digest) => Ok((encode_digest(digest), "cs")),
        Key::TxToCheckpoint(digest) => Ok((encode_digest(digest), "tx2c")),
        Key::ObjectKey(object_id, version) => Ok((encode_object_key(object_id, version), "ob")),
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

        let client = Client::builder().http2_prior_knowledge().build().unwrap();

        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{}/", base_url)
        };

        let base_url = Url::parse(&base_url).into_sui_result()?;

        Ok(Self { base_url, client })
    }

    fn get_url(&self, key: &Key) -> SuiResult<Url> {
        let (digest, item_type) = key_to_path_elements(key)?;
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
            resp.bytes().await.map(Some).into_sui_result()
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

    #[instrument(level = "trace", skip_all)]
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
            .chain(
                checkpoint_contents_by_digest
                    .iter()
                    .map(|cp| Key::CheckpointContentsByDigest(*cp)),
            )
            .collect::<Vec<_>>();

        let summaries_len = checkpoint_summaries.len();
        let contents_len = checkpoint_contents.len();
        let summaries_by_digest_len = checkpoint_summaries_by_digest.len();
        let contents_by_digest_len = checkpoint_contents_by_digest.len();

        let fetches = self.multi_fetch(keys).await;

        let input_slices = [
            summaries_len,
            contents_len,
            summaries_by_digest_len,
            contents_by_digest_len,
        ];

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

        let contents_by_digest_results = result_slices[3]
            .iter()
            .zip(checkpoint_contents_by_digest.iter())
            .map(map_fetch)
            .map(|maybe_bytes| {
                maybe_bytes.and_then(|(bytes, digest)| {
                    deser_check_digest(digest, bytes, |c: &CheckpointContents| *c.digest())
                })
            })
            .collect::<Vec<_>>();

        Ok((
            summaries_results,
            contents_results,
            summaries_by_digest_results,
            contents_by_digest_results,
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
        self.fetch(key)
            .await
            .map(|maybe| maybe.and_then(|bytes| deser::<_, Object>(&key, bytes.as_ref())))
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
}
