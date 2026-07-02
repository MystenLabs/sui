// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test helpers for the streaming submodule.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::anyhow;
use dashmap::DashMap;
use sui_rpc::field::FieldMask;
use sui_rpc::proto::sui::rpc::v2 as grpc;
use sui_rpc::proto::sui::rpc::v2::Checkpoint as ProtoCheckpoint;
use sui_sdk_types::Bitmap;
use sui_sdk_types::Bls12381Signature;
use sui_sdk_types::ValidatorAggregatedSignature as SdkValidatorAggregatedSignature;
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::gas::GasCostSummary;
use sui_types::messages_checkpoint::CheckpointContents as NativeCheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary as NativeCheckpointSummary;
use tokio::sync::broadcast;

use super::checkpoint_stream_task::SubscriptionBroadcast;
use super::gap_recovery::CheckpointFetcher;
use super::processed_checkpoint::ProcessedCheckpoint;

/// Per-key behavior of the mock fetcher.
#[derive(Debug, Clone)]
pub(super) enum FetcherBehavior {
    /// Always return `Ok(Some(make_test_proto_checkpoint(seq)))`.
    Success,
    /// Return `Err` for the first N calls, then `Ok(Some(...))` afterward.
    ErrorThenSuccess(usize),
    /// Return `Ok(None)` for the first N calls, then `Ok(Some(...))` afterward.
    NoneThenSuccess(usize),
}

/// Mock fetcher with per-seq behavior, tracking call counts. Panics on unconfigured seqs so
/// tests fail loudly if unexpected fetches happen. `Clone` shares the underlying state, so
/// call counts are aggregated across clones (`scan_checkpoints` clones the fetcher per item).
#[derive(Clone)]
pub(super) struct MockFetcher {
    state: Arc<DashMap<u64, (FetcherBehavior, usize)>>,
}

impl MockFetcher {
    pub(super) fn new(setup: HashMap<u64, FetcherBehavior>) -> Self {
        Self {
            state: Arc::new(setup.into_iter().map(|(k, v)| (k, (v, 0))).collect()),
        }
    }

    /// Build a fetcher from a slice of (seq, behavior) pairs.
    pub(super) fn from_setup(setup: &[(u64, FetcherBehavior)]) -> Self {
        Self::new(setup.iter().cloned().collect())
    }

    /// Build a fetcher that returns `Success` for every seq in the given inclusive range.
    pub(super) fn success_for_range(range: std::ops::RangeInclusive<u64>) -> Self {
        let setup = range.map(|seq| (seq, FetcherBehavior::Success)).collect();
        Self::new(setup)
    }

    pub(super) fn calls_for(&self, seq: u64) -> usize {
        self.state.get(&seq).map_or(0, |g| g.1)
    }
}

impl CheckpointFetcher for MockFetcher {
    async fn fetch_checkpoint(
        &self,
        seq: u64,
        _mask: &FieldMask,
    ) -> anyhow::Result<Option<ProtoCheckpoint>> {
        let (behavior, calls) = {
            let mut entry = self
                .state
                .get_mut(&seq)
                .unwrap_or_else(|| panic!("MockFetcher: unconfigured key {seq}"));
            entry.1 += 1;
            (entry.0.clone(), entry.1)
        };

        match behavior {
            FetcherBehavior::Success => Ok(Some(make_test_proto_checkpoint(seq))),
            FetcherBehavior::ErrorThenSuccess(n) => {
                if calls <= n {
                    Err(anyhow!("simulated transient error for cp {seq}"))
                } else {
                    Ok(Some(make_test_proto_checkpoint(seq)))
                }
            }
            FetcherBehavior::NoneThenSuccess(n) => {
                if calls <= n {
                    Ok(None)
                } else {
                    Ok(Some(make_test_proto_checkpoint(seq)))
                }
            }
        }
    }
}

/// Build a `SubscriptionBroadcast` with the given `first_live_checkpoint` and a buffer large
/// enough that tests do not trigger lag incidentally. Returns the sender so tests can drive
/// the channel directly to advance `network_tip()`.
pub(super) fn test_broadcast(
    first_live_checkpoint: u64,
) -> (
    broadcast::Sender<Arc<ProcessedCheckpoint>>,
    Arc<SubscriptionBroadcast>,
) {
    let (tx, rx) = broadcast::channel(256);
    (
        tx,
        Arc::new(SubscriptionBroadcast::new(rx, first_live_checkpoint)),
    )
}

/// Build a fully deserializable test `ProtoCheckpoint` at the given sequence number.
/// Empty contents, default aggregate signature. `process_checkpoint` parses (but does not
/// verify) the signature, so the default BLS bytes are accepted.
pub(super) fn make_test_proto_checkpoint(seq: u64) -> ProtoCheckpoint {
    let contents = NativeCheckpointContents::new_with_digests_only_for_tests(vec![]);
    let summary = NativeCheckpointSummary {
        epoch: 0,
        sequence_number: seq,
        network_total_transactions: 0,
        content_digest: *contents.digest(),
        previous_digest: None,
        epoch_rolling_gas_cost_summary: GasCostSummary::default(),
        timestamp_ms: 0,
        checkpoint_commitments: vec![],
        end_of_epoch_data: None,
        version_specific_data: vec![],
    };
    // Default bytes are the G1 infinity point, which round-trips through
    // `AggregateAuthoritySignature::from_bytes` (all-zero bytes do not).
    let sig_bytes: [u8; 48] = AggregateAuthoritySignature::default()
        .as_ref()
        .try_into()
        .unwrap();
    let sdk_sig = SdkValidatorAggregatedSignature {
        epoch: 0,
        signature: Bls12381Signature::new(sig_bytes),
        bitmap: Bitmap::default(),
    };

    let mut summary_bcs = grpc::Bcs::default();
    summary_bcs.value = Some(bcs::to_bytes(&summary).unwrap().into());
    let mut summary_proto = grpc::CheckpointSummary::default();
    summary_proto.bcs = Some(summary_bcs);

    let mut contents_bcs = grpc::Bcs::default();
    contents_bcs.value = Some(bcs::to_bytes(&contents).unwrap().into());
    let mut contents_proto = grpc::CheckpointContents::default();
    contents_proto.bcs = Some(contents_bcs);

    let mut cp = ProtoCheckpoint::default();
    cp.sequence_number = Some(seq);
    cp.summary = Some(summary_proto);
    cp.contents = Some(contents_proto);
    cp.signature = Some(sdk_sig.into());
    cp
}
