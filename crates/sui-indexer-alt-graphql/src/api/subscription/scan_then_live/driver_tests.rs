// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ordering of matches and progress through the real `subscribe` loop: a scripted ledger feeds the
//! backfill, and checkpoints sent on a real broadcast channel feed the live phase.

use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::Mutex;

use futures::StreamExt;
use prometheus::Registry;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
use sui_indexer_alt_reader::package_resolver::DbPackageStore;
use sui_indexer_alt_reader::package_resolver::PackageCache;
use sui_indexer_alt_reader::pg_reader::PgReader;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::Position;

use crate::api::types::checkpoint::CCheckpoint;
use crate::metrics::SubscriptionMetrics;
use crate::task::streaming::StreamedObjectStore;
use crate::task::streaming::StreamedPackageStore;
use crate::task::streaming::StreamedTransactionStore;
use crate::task::streaming::process_checkpoint;
use crate::task::streaming::test_utils::make_test_proto_checkpoint;

use super::*;

/// A feed over checkpoint cursors whose ledger pages are scripted and whose live matches are a
/// fixed set of checkpoints. Each match's node is its checkpoint.
struct Feed;

struct Script {
    pages: Mutex<VecDeque<StreamPage<u64>>>,
    live_matches: BTreeSet<u64>,
}

impl Subscribable for Feed {
    type Item = u64;
    type Cursor = CCheckpoint;
    type Filter = Arc<Script>;
    type ScanItem = u64;

    fn scan<'a>(
        _reader: &'a AlphaLedgerGrpcReader,
        _cp_bounds: RangeInclusive<u64>,
        _page: &'a Page<CCheckpoint>,
        script: &'a Arc<Script>,
    ) -> impl Future<Output = Result<StreamPage<u64>, RpcError>> + Send + 'a {
        let next = script.pages.lock().unwrap().pop_front();
        async move {
            match next {
                Some(page) => Ok(page),
                // Nothing more indexed: the driver must have stopped scanning by now.
                None => std::future::pending().await,
            }
        }
    }

    fn build_node(
        _: &Arc<StreamedCaches>,
        _: &sui_package_resolver::Limits,
        checkpoint: &u64,
    ) -> Result<u64, RpcError> {
        Ok(*checkpoint)
    }

    fn matching_edges(
        checkpoint: &Arc<ProcessedCheckpoint>,
        _: &Arc<StreamedCaches>,
        _: &sui_package_resolver::Limits,
        script: &Arc<Script>,
    ) -> Result<Vec<Edge<String, u64, EmptyFields>>, RpcError> {
        let seq = checkpoint.summary.sequence_number;
        Ok(if script.live_matches.contains(&seq) {
            vec![Edge::new(item(seq).encode_cursor(), seq)]
        } else {
            vec![]
        })
    }

    const BOUNDARY_AFTER: Option<fn(&ProcessedCheckpoint) -> CCheckpoint> =
        Some(|checkpoint| boundary(checkpoint.summary.sequence_number + 1));

    fn subscription_type() -> &'static str {
        "driver_test"
    }
}

/// What the subscriber received, by checkpoint: a match, or progress resuming at a checkpoint.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Match(u64),
    Progress(u64),
}

fn token(position: u64, kind: CursorKind) -> CursorToken {
    let position = Position::Checkpoints {
        checkpoint: position,
    };
    match kind {
        CursorKind::Item => CursorToken::item(position),
        _ => CursorToken::boundary(position),
    }
}

fn item(checkpoint: u64) -> CCheckpoint {
    CCheckpoint::try_from(token(checkpoint, CursorKind::Item)).unwrap()
}

fn boundary(checkpoint: u64) -> CCheckpoint {
    CCheckpoint::try_from(token(checkpoint, CursorKind::Boundary)).unwrap()
}

/// A ledger page with matches at `matches`, ending with a `Boundary` cursor at `end`.
fn page(matches: &[u64], end: u64) -> StreamPage<u64> {
    let items = matches
        .iter()
        .map(|&checkpoint| PageItem {
            payload: checkpoint,
            cursor: token(checkpoint, CursorKind::Item).encode(),
        })
        .collect();
    StreamPage::for_test(
        items,
        None,
        Some(token(end, CursorKind::Boundary).encode()),
        None,
    )
}

/// Classify a delivery, checking that a match carries an `Item` cursor at its own checkpoint and
/// progress a `Boundary` cursor.
fn seen(delivery: Delivery<u64>) -> Seen {
    let (cursor, node) = delivery.into_parts();
    let token = CursorToken::from(&CCheckpoint::decode_cursor(&cursor).unwrap());
    let checkpoint = token.position.checkpoint();
    match node {
        Some(node) => {
            assert_eq!(token.kind, CursorKind::Item);
            assert_eq!(node, checkpoint);
            Seen::Match(checkpoint)
        }
        None => {
            assert_eq!(token.kind, CursorKind::Boundary);
            Seen::Progress(checkpoint)
        }
    }
}

struct Harness {
    sender: broadcast::Sender<Arc<ProcessedCheckpoint>>,
    stream: std::pin::Pin<Box<dyn Stream<Item = Result<Delivery<u64>, RpcError>> + Send>>,
}

impl Harness {
    /// Subscribe `after` an item cursor at `after`, with the streamed tip at `tip`, the handoff
    /// pinned within 2 checkpoints of it, and live progress after 2 quiet checkpoints.
    async fn new(after: u64, tip: u64, pages: Vec<StreamPage<u64>>, live: &[u64]) -> Self {
        let (sender, template) = broadcast::channel(64);
        // The tip is read from the template receiver's backlog, so send through `tip` first.
        let first_live = tip - 2;
        for seq in first_live..=tip {
            send(&sender, seq);
        }
        let broadcast = Arc::new(SubscriptionBroadcast::new(
            template,
            first_live,
            SubscriptionMetrics::new_for_test(),
        ));
        assert_eq!(broadcast.network_tip(), tip);

        let pg = PgReader::new(None, None, DbArgs::default(), &Registry::new())
            .await
            .unwrap();
        let packages = Arc::new(PackageCache::new(DbPackageStore::new(Arc::new(
            pg.as_data_loader(),
        ))));
        let caches = Arc::new(StreamedCaches::new(
            Arc::new(StreamedPackageStore::new(packages)),
            Arc::new(StreamedTransactionStore::new()),
            Arc::new(StreamedObjectStore::new()),
        ));

        let (_, watermarks_rx) =
            watch::channel(Arc::new(Watermarks::for_test(&[("pipeline", 1_000_000)])));
        let script = Arc::new(Script {
            pages: Mutex::new(pages.into()),
            live_matches: live.iter().copied().collect(),
        });
        let config = SubscriptionConfig {
            broadcast_buffer: 4,
            max_concurrent_resolutions: 10,
            progress_quiet_checkpoints: 2,
            ..SubscriptionConfig::default()
        };

        let stream = subscribe::<Feed>(
            unused_reader().await,
            broadcast,
            caches,
            crate::config::Limits::default().package_resolver(),
            watermarks_rx,
            script,
            Some(item(after)),
            None,
            SubscriberLimit::new(8),
            config,
        )
        .unwrap_or_else(|_| panic!("subscribe rejected"));

        Self {
            sender,
            stream: Box::pin(stream),
        }
    }

    /// The next `n` deliveries.
    async fn next(&mut self, n: usize) -> Vec<Seen> {
        let mut out = vec![];
        for _ in 0..n {
            let delivery = tokio::time::timeout(Duration::from_secs(5), self.stream.next())
                .await
                .expect("timed out waiting for a delivery")
                .expect("stream ended")
                .unwrap_or_else(|e| panic!("delivery failed: {e:?}"));
            out.push(seen(delivery));
        }
        out
    }

    /// Broadcast checkpoints `range` live.
    fn send_live(&self, range: RangeInclusive<u64>) {
        for seq in range {
            send(&self.sender, seq);
        }
    }

    /// Nothing more is delivered for a moment.
    async fn assert_idle(&mut self) {
        let next = tokio::time::timeout(Duration::from_millis(200), self.stream.next()).await;
        assert!(
            next.is_err(),
            "unexpected delivery: {:?}",
            next.unwrap().map(|d| d.map(seen))
        );
    }
}

fn send(sender: &broadcast::Sender<Arc<ProcessedCheckpoint>>, seq: u64) {
    let checkpoint = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
    // A send fails only with no receivers, and the template receiver lives as long as the broadcast.
    let _ = sender.send(Arc::new(checkpoint));
}

/// A reader that is never called: `Feed` answers every scan itself.
async fn unused_reader() -> AlphaLedgerGrpcReader {
    let args = LedgerGrpcArgs {
        ledger_grpc_statement_timeout_ms: None,
        ledger_grpc_max_decoding_message_size: 1024,
        ledger_grpc_num_connections: 1,
    };
    AlphaLedgerGrpcReader::new(
        "http://localhost:1".parse().unwrap(),
        args,
        None,
        &Registry::new(),
        None,
    )
    .await
    .unwrap()
}

/// A page's end goes out as progress only after the page's matches. The page that reaches the
/// pinned handoff sends its end as progress and then ends the scan, and live takes over after it:
/// a quiet run sends progress, and a later match follows it.
#[tokio::test]
async fn progress_follows_page_matches_then_scan_ends() {
    let mut h = Harness::new(
        3,
        10,
        // Matches at 5 and 6, scanned through 7; then nothing more through the tip.
        vec![page(&[5, 6], 8), page(&[], 11)],
        &[13],
    )
    .await;

    assert_eq!(
        h.next(4).await,
        vec![
            Seen::Match(5),
            Seen::Match(6),
            Seen::Progress(8),
            // Covers through 10, the pinned handoff: the scan ends here.
            Seen::Progress(11),
        ]
    );

    // Live from 11: two quiet checkpoints send progress at the start of 13, then 13 matches.
    h.send_live(11..=13);
    assert_eq!(h.next(2).await, vec![Seen::Progress(13), Seen::Match(13)]);
    h.assert_idle().await;
}

/// The page that reaches the handoff scanned past it (a ledger ahead of this replica's stream). Its
/// end goes out as progress, so live delivers nothing at or below it, not even a match the stream
/// reports there, and its quiet run starts after it.
#[tokio::test]
async fn page_ending_past_the_pin_bounds_live() {
    let mut h = Harness::new(3, 10, vec![page(&[9], 16)], &[13, 18]).await;

    // The match at 9 pins the handoff at the tip, 10; the page covers through 15.
    assert_eq!(h.next(2).await, vec![Seen::Match(9), Seen::Progress(16)]);

    // 11 through 15 are covered (13's live match is dropped); 16 and 17 are a quiet run; 18
    // matches.
    h.send_live(11..=18);
    assert_eq!(h.next(2).await, vec![Seen::Progress(18), Seen::Match(18)]);
    h.assert_idle().await;
}
