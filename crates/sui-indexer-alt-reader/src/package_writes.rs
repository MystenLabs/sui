// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A write-granularity view over the LedgerService's transaction-granularity scan: list every
//! Move package write (publishes and upgrades) in a checkpoint range, in transaction order.
//!
//! The wire only addresses transactions (the `AnyPackageWrite` bitmap dimension), so this module
//! bridges the granularity gap client-side: it scans matching transactions, expands each into its
//! package writes from the effects, trims to the requested limit, and re-mints the result as a
//! write-granularity [`StreamPage`] — one item per package write, with [`PackageToken`] cursors,
//! and scan frontiers in the watermark slots. Consumers can treat it exactly like a page drained
//! from a write-granularity list API.

use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use anyhow::Context;
use bytes::Bytes;
use prost::Message;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;

use crate::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use crate::alpha_ledger_grpc_reader::PageItem;
use crate::alpha_ledger_grpc_reader::StreamPage;

/// One package write served by the scan: the storage ID and version the effects reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageWrite {
    pub id: ObjectID,
    pub version: u64,
}

/// Resume token for a package-write scan. Points at one package write within one transaction, or
/// at scan progress through a transaction that served nothing.
#[derive(PartialEq, Eq, Clone, Debug, Copy)]
pub struct PackageToken {
    /// Hint for the checkpoint the transaction belongs to (0 = unknown).
    checkpoint: u64,
    position: PackagePosition,
}

/// Where a [`PackageToken`] sits in the scan. Enriches the wire's item vs. watermark cursor
/// distinction: a cursor either points at a served package write, or at scan progress through a
/// transaction that served nothing.
#[derive(PartialEq, Eq, Clone, Debug, Copy)]
enum PackagePosition {
    /// Points at one package write within a transaction, by its position in effects order. The
    /// transaction may hold writes on either side of this one, so resuming from here re-fetches
    /// the transaction and skips the writes on the cursor's served side (at-or-before it going
    /// forwards, at-or-after it going backwards).
    Tx { tx_seq: u64, write_index: u32 },

    /// A scan frontier: the wire scanned through this transaction but served nothing from it.
    /// Resume strictly past it, in either direction.
    Scan { tx_seq: u64 },
}

/// Wire format of [`PackageToken`], following `sui-rpc-cursor`'s pattern of a protobuf message
/// behind a native type. This format is minted and consumed by this module only — it is not part
/// of the gRPC vocabulary — but a server-side write-granularity list API could adopt it (or lift
/// it into `sui-rpc-cursor`) without reshaping it. Field numbering is append-only.
#[derive(Message)]
struct WireToken {
    #[prost(uint64, optional, tag = "1")]
    checkpoint: Option<u64>,

    #[prost(uint64, optional, tag = "2")]
    tx_seq: Option<u64>,

    /// Present for a write position (`Tx`); absent for a scan frontier (`Scan`).
    #[prost(uint32, optional, tag = "3")]
    write_index: Option<u32>,
}

impl PackageToken {
    /// Encode into the proto wire format.
    pub fn encode(&self) -> Bytes {
        let (tx_seq, write_index) = match self.position {
            PackagePosition::Tx {
                tx_seq,
                write_index,
            } => (tx_seq, Some(write_index)),
            PackagePosition::Scan { tx_seq } => (tx_seq, None),
        };

        WireToken {
            checkpoint: Some(self.checkpoint),
            tx_seq: Some(tx_seq),
            write_index,
        }
        .encode_to_vec()
        .into()
    }

    /// Decode from the proto wire format.
    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let wire = WireToken::decode(bytes).context("Failed to decode package token")?;
        let checkpoint = wire.checkpoint.context("Package token missing checkpoint")?;
        let tx_seq = wire.tx_seq.context("Package token missing tx_seq")?;
        let position = match wire.write_index {
            Some(write_index) => PackagePosition::Tx {
                tx_seq,
                write_index,
            },
            None => PackagePosition::Scan { tx_seq },
        };

        Ok(PackageToken {
            checkpoint,
            position,
        })
    }

    /// Converts an encoded wire `CursorToken` (a transaction position) into a scan frontier.
    /// Refined to a write position with [`Self::at`] when the transaction turns out to have
    /// served one.
    fn from_stream_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        let token = CursorToken::decode(bytes).context("Failed to decode stream cursor")?;
        let Position::Transactions { checkpoint, tx_seq } = token.position else {
            anyhow::bail!("Unexpected position in stream cursor");
        };
        Ok(PackageToken {
            checkpoint,
            position: PackagePosition::Scan { tx_seq },
        })
    }

    /// The position of one package write within this token's transaction.
    fn at(self, write_index: u32) -> Self {
        Self {
            checkpoint: self.checkpoint,
            position: PackagePosition::Tx {
                tx_seq: self.tx_seq(),
                write_index,
            },
        }
    }

    fn tx_seq(&self) -> u64 {
        match self.position {
            PackagePosition::Tx { tx_seq, .. } | PackagePosition::Scan { tx_seq } => tx_seq,
        }
    }

    /// Whether the write at `write_index` of transaction `tx_seq` is behind this token when it is
    /// used as an `after` bound.
    fn covers_up_to(&self, tx_seq: u64, write_index: u32) -> bool {
        self.tx_seq() == tx_seq
            && match self.position {
                PackagePosition::Tx {
                    write_index: served,
                    ..
                } => write_index <= served,
                PackagePosition::Scan { .. } => true,
            }
    }

    /// Whether the write at `write_index` of transaction `tx_seq` is behind this token when it is
    /// used as a `before` bound.
    fn covers_from(&self, tx_seq: u64, write_index: u32) -> bool {
        self.tx_seq() == tx_seq
            && match self.position {
                PackagePosition::Tx {
                    write_index: served,
                    ..
                } => write_index >= served,
                PackagePosition::Scan { .. } => true,
            }
    }
}

impl AlphaLedgerGrpcReader {
    /// List package writes in `cp_bounds` (inclusive), in transaction order, `ascending` or
    /// descending, starting from the optional `after`/`before` resume tokens.
    ///
    /// At most `limit` writes are returned, one [`StreamPage`] item per write, each carrying an
    /// encoded [`PackageToken`] cursor. Scan frontiers that are not an item's own position occupy
    /// the page's watermark slots, so `first_cursor()`/`last_cursor()` are always valid resume
    /// points; a page trimmed client-side reports `has_more()`.
    pub async fn list_package_writes(
        &self,
        cp_bounds: RangeInclusive<u64>,
        limit: usize,
        ascending: bool,
        after: Option<PackageToken>,
        before: Option<PackageToken>,
    ) -> anyhow::Result<StreamPage<PackageWrite>> {
        let request = scan_request(
            &cp_bounds,
            limit,
            ascending,
            after.as_ref(),
            before.as_ref(),
        );
        let result = self.list_transactions(request).await?;
        let expanded = expand_package_writes(ascending, after, before, &result.items)?;
        paginate_package_writes(limit, expanded, &result)
    }
}

/// Build the `ListTransactions` request scanning `cp_bounds` for package-writing transactions.
///
/// The wire cursors address transactions, so a `Tx` cursor widens to re-include its transaction —
/// the writes it has already covered are skipped client-side during expansion — while a `Scan`
/// cursor bounds on its transaction directly. Checkpoint hints on widened bounds are reset to
/// unknown (0 for `after`, `u64::MAX` for `before`) because the neighboring transaction may fall
/// in a different checkpoint.
fn scan_request(
    cp_bounds: &RangeInclusive<u64>,
    limit: usize,
    ascending: bool,
    after: Option<&PackageToken>,
    before: Option<&PackageToken>,
) -> proto::ListTransactionsRequest {
    let wire_after = after.and_then(|t| {
        let position = match t.position {
            // Nothing was served from a scan frontier; bound on it directly.
            PackagePosition::Scan { tx_seq } => Position::Transactions {
                checkpoint: t.checkpoint,
                tx_seq,
            },
            // Nothing precedes the first transaction; resume from the range start and skip
            // client-side.
            PackagePosition::Tx { tx_seq: 0, .. } => return None,
            // The cursor's transaction may hold further writes; re-include it.
            PackagePosition::Tx { tx_seq, .. } => Position::Transactions {
                checkpoint: 0,
                tx_seq: tx_seq - 1,
            },
        };
        Some(CursorToken::item(position).encode())
    });

    let wire_before = before.map(|t| {
        let position = match t.position {
            // A scan frontier, or a cursor at a transaction's first write: everything from the
            // transaction's start onward is on the served side, so bound on it directly.
            PackagePosition::Scan { tx_seq }
            | PackagePosition::Tx {
                tx_seq,
                write_index: 0,
            } => Position::Transactions {
                // A 0 hint would collapse the checkpoint window, so treat it as unknown.
                checkpoint: if t.checkpoint == 0 {
                    u64::MAX
                } else {
                    t.checkpoint
                },
                tx_seq,
            },
            // Writes before the cursor still belong to the backward page; re-include the
            // transaction.
            PackagePosition::Tx { tx_seq, .. } => Position::Transactions {
                checkpoint: u64::MAX,
                tx_seq: tx_seq.saturating_add(1),
            },
        };
        CursorToken::item(position).encode()
    });

    let mut options = proto::QueryOptions::default();
    // One extra transaction: a re-included boundary transaction may contribute no further
    // packages, while every other matched transaction contributes at least one.
    options.limit = Some(limit as u32 + 1);
    options.after = wire_after;
    options.before = wire_before;
    options.ordering = Some(if ascending {
        proto::Ordering::Ascending as i32
    } else {
        proto::Ordering::Descending as i32
    });

    let mut request = proto::ListTransactionsRequest::default();
    // Only the effects are needed to identify package writes; contents load separately.
    request.read_mask = Some(prost_types::FieldMask::from_paths(["effects.bcs"]));
    request.start_checkpoint = Some(*cp_bounds.start());
    // `cp_bounds` end is inclusive; the request bound is exclusive.
    request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
    request.filter = Some(package_write_filter());
    request.options = Some(options);
    request
}

/// Expand a page of package-writing transactions into the package writes they serve, in scan
/// order, skipping writes a boundary cursor has already covered — their transactions were
/// deliberately re-included by the widened wire bounds.
fn expand_package_writes(
    ascending: bool,
    after: Option<PackageToken>,
    before: Option<PackageToken>,
    items: &[PageItem<proto::ExecutedTransaction>],
) -> anyhow::Result<Vec<(PackageToken, PackageWrite)>> {
    let mut expanded = vec![];
    for item in items {
        let position = PackageToken::from_stream_cursor(&item.cursor)?;

        let mut writes = package_writes(&item.payload)?;
        if !ascending {
            writes.reverse();
        }

        for (write_index, write) in writes {
            if after.is_some_and(|a| a.covers_up_to(position.tx_seq(), write_index)) {
                continue;
            }
            if before.is_some_and(|b| b.covers_from(position.tx_seq(), write_index)) {
                continue;
            }

            expanded.push((position.at(write_index), write));
        }
    }

    Ok(expanded)
}

/// Re-mint expanded package writes as a write-granularity [`StreamPage`]: trim to `limit`, one
/// item per kept write, and place the scan fences.
///
/// A fence naming the same transaction as its boundary write is that item's own cursor — the
/// transaction served writes, so the item cursor is the resume point and the watermark slot stays
/// empty. Otherwise nothing was served from the fence's transaction, and it occupies the
/// watermark slot as a `Scan` frontier. A trimmed page drops the far fence (its resume point is
/// the last kept item) and folds the trim into the end reason, so `has_more()` reports it.
fn paginate_package_writes(
    limit: usize,
    mut expanded: Vec<(PackageToken, PackageWrite)>,
    result: &StreamPage<proto::ExecutedTransaction>,
) -> anyhow::Result<StreamPage<PackageWrite>> {
    // More matches than the page can hold: trim the scan-direction tail and resume from the last
    // write kept rather than the scan frontier.
    let trimmed = expanded.len() > limit;
    expanded.truncate(limit);

    let first_pos = result
        .first_cursor()
        .map(|c| PackageToken::from_stream_cursor(c))
        .transpose()?;
    let last_pos = result
        .last_cursor()
        .map(|c| PackageToken::from_stream_cursor(c))
        .transpose()?;

    let first_edge = expanded.first().map(|(t, _)| *t);
    let last_edge = expanded.last().map(|(t, _)| *t);

    let same_tx = |edge: Option<PackageToken>, fence: Option<PackageToken>| {
        edge.zip(fence)
            .is_some_and(|(edge, fence)| edge.tx_seq() == fence.tx_seq())
    };

    let first_wm_cursor = if same_tx(first_edge, first_pos) {
        None
    } else {
        first_pos.map(|t| t.encode())
    };

    let last_wm_cursor = if trimmed || same_tx(last_edge, last_pos) {
        None
    } else {
        last_pos.map(|t| t.encode())
    };

    let end_reason = if trimmed {
        // The page was cut client-side: morally an item-limit end, which `has_more()` reports as
        // more results remaining.
        Some(proto::QueryEndReason::ItemLimit)
    } else {
        result.end_reason
    };

    let items = expanded
        .into_iter()
        .map(|(token, write)| PageItem {
            payload: write,
            cursor: token.encode(),
        })
        .collect();

    Ok(StreamPage::from_parts(
        items,
        first_wm_cursor,
        last_wm_cursor,
        end_reason,
    ))
}

/// Extract a transaction's package writes from its effects: the written refs of published
/// packages, paired with their index in effects order (`written()` preserves it), which
/// `PackageToken`'s write index is defined over.
fn package_writes(
    transaction: &proto::ExecutedTransaction,
) -> anyhow::Result<Vec<(u32, PackageWrite)>> {
    let effects: TransactionEffects = transaction
        .effects
        .as_ref()
        .and_then(|fx| fx.bcs.as_ref())
        .context("ListTransactions item missing effects")?
        .deserialize()
        .context("Failed to deserialize effects")?;

    let published: BTreeSet<ObjectID> = effects.published_packages().into_iter().collect();
    Ok(effects
        .written()
        .into_iter()
        .filter(|(id, _, _)| published.contains(id))
        .enumerate()
        .map(|(i, (id, version, _))| {
            (
                i as u32,
                PackageWrite {
                    id,
                    version: version.value(),
                },
            )
        })
        .collect())
}

/// A filter matching every transaction that wrote a Move package — first publishes and upgrades
/// alike (the `AnyPackageWrite` bitmap dimension).
fn package_write_filter() -> proto::TransactionFilter {
    let mut literal = proto::TransactionLiteral::default();
    literal.predicate = Some(proto::transaction_literal::Predicate::PackageWrite(
        proto::PackageWriteFilter::default(),
    ));

    proto::TransactionFilter::default().with_terms(vec![
        proto::TransactionTerm::default().with_literals(vec![literal]),
    ])
}

#[cfg(test)]
mod tests {
    use sui_types::base_types::SequenceNumber;
    use sui_types::base_types::SuiAddress;
    use sui_types::base_types::random_object_ref;
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::SenderSignedData;
    use sui_types::transaction::TransactionData;

    use super::*;

    /// The checkpoint hint carried by item cursors in these tests.
    const CP: u64 = 7;

    fn pkg(n: u8) -> ObjectID {
        ObjectID::from_single_byte(n)
    }

    /// Wire cursor bytes for transaction `tx_seq` in `checkpoint`.
    fn wire_cursor(checkpoint: u64, tx_seq: u64) -> Bytes {
        CursorToken::item(Position::Transactions { checkpoint, tx_seq }).encode()
    }

    /// A `PageItem` whose effects publish `packages` as `(id, version)` pairs, with its cursor at
    /// `(CP, tx_seq)`.
    fn pkg_item(tx_seq: u64, packages: &[(ObjectID, u64)]) -> PageItem<proto::ExecutedTransaction> {
        let pt = ProgrammableTransactionBuilder::new().finish();
        let data = TransactionData::new_programmable(
            SuiAddress::ZERO,
            vec![random_object_ref()],
            pt,
            1,
            1,
        );
        let signed = SenderSignedData::new(data, vec![]);

        let effects = TestEffectsBuilder::new(&signed)
            .with_package_writes(
                packages
                    .iter()
                    .map(|(id, version)| (*id, SequenceNumber::from(*version))),
            )
            .build();

        let mut fx = proto::TransactionEffects::default();
        fx.bcs = Some(proto::Bcs::serialize(&effects).expect("serialize effects"));

        let mut payload = proto::ExecutedTransaction::default();
        payload.effects = Some(fx);

        PageItem {
            payload,
            cursor: wire_cursor(CP, tx_seq),
        }
    }

    /// A `Tx` token in checkpoint `CP`.
    fn tx(tx_seq: u64, write_index: u32) -> PackageToken {
        PackageToken {
            checkpoint: CP,
            position: PackagePosition::Tx {
                tx_seq,
                write_index,
            },
        }
    }

    fn scan(checkpoint: u64, tx_seq: u64) -> PackageToken {
        PackageToken {
            checkpoint,
            position: PackagePosition::Scan { tx_seq },
        }
    }

    /// Expand and page-ify in one step, as [`AlphaLedgerGrpcReader::list_package_writes`]
    /// composes them.
    fn paginate(
        limit: usize,
        ascending: bool,
        after: Option<PackageToken>,
        before: Option<PackageToken>,
        result: &StreamPage<proto::ExecutedTransaction>,
    ) -> StreamPage<PackageWrite> {
        let expanded =
            expand_package_writes(ascending, after, before, &result.items).expect("expand");
        paginate_package_writes(limit, expanded, result).expect("paginate")
    }

    /// The served items, flattened to `(tx_seq, write_index, id, version)` by decoding each
    /// item's cursor.
    fn served(page: &StreamPage<PackageWrite>) -> Vec<(u64, u32, ObjectID, u64)> {
        page.items
            .iter()
            .map(|item| {
                let token = PackageToken::decode(&item.cursor).expect("decode item cursor");
                let PackagePosition::Tx {
                    tx_seq,
                    write_index,
                } = token.position
                else {
                    panic!("item cursor must be a `Tx` position");
                };
                (tx_seq, write_index, item.payload.id, item.payload.version)
            })
            .collect()
    }

    /// Decode a synthesized page cursor back into its `PackageToken`.
    fn page_cursor(cursor: Option<&Bytes>) -> PackageToken {
        PackageToken::decode(cursor.expect("cursor present")).expect("decode page cursor")
    }

    /// Decode a wire bound back into `(checkpoint, tx_seq)`.
    fn wire_position(bytes: &Bytes) -> (u64, u64) {
        let token = CursorToken::decode(bytes).expect("decode wire cursor");
        let Position::Transactions { checkpoint, tx_seq } = token.position else {
            panic!("expected transactions position, got {:?}", token.position);
        };
        (checkpoint, tx_seq)
    }

    /// The token wire format round-trips both variants.
    #[test]
    fn token_round_trip() {
        for token in [tx(42, 3), tx(0, 0), scan(9, 17), scan(0, 0)] {
            assert_eq!(
                PackageToken::decode(&token.encode()).expect("decode"),
                token
            );
        }
    }

    /// Fences that are the boundary items' own cursors leave the watermark slots empty: the
    /// items' cursors are the resume points.
    #[test]
    fn forward_fences_on_items_mint_tx() {
        let (a, b) = (pkg(1), pkg(2));
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1)]), pkg_item(11, &[(b, 1)])],
            None,
            None,
            Some(proto::QueryEndReason::LedgerTip),
        );

        let page = paginate(5, true, None, None, &result);

        assert_eq!(served(&page), vec![(10, 0, a, 1), (11, 0, b, 1)]);
        assert_eq!(page_cursor(page.first_cursor()), tx(10, 0));
        assert_eq!(page_cursor(page.last_cursor()), tx(11, 0));
        assert!(!page.has_more());
    }

    /// Standalone watermark cursors re-mint as `Scan` fences in the watermark slots: their
    /// transactions served nothing, so resumption seeks strictly past them.
    #[test]
    fn forward_watermark_fences_mint_scan() {
        let a = pkg(1);
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1)])],
            Some(wire_cursor(6, 8)),
            Some(wire_cursor(9, 15)),
            None,
        );

        let page = paginate(5, true, None, None, &result);

        assert_eq!(served(&page), vec![(10, 0, a, 1)]);
        assert_eq!(page_cursor(page.first_cursor()), scan(6, 8));
        assert_eq!(page_cursor(page.last_cursor()), scan(9, 15));
        assert!(page.has_more());
    }

    /// A trimmed page resumes from its last kept write, not the scan frontier, and reports more
    /// results remaining.
    #[test]
    fn forward_trim_resumes_from_last_kept_write() {
        let (a, b, c, d) = (pkg(1), pkg(2), pkg(3), pkg(4));
        let result = StreamPage::for_test(
            vec![
                pkg_item(10, &[(a, 1), (b, 1)]),
                pkg_item(11, &[(c, 1), (d, 1)]),
            ],
            None,
            None,
            Some(proto::QueryEndReason::LedgerTip),
        );

        let page = paginate(3, true, None, None, &result);

        assert_eq!(
            served(&page),
            vec![(10, 0, a, 1), (10, 1, b, 1), (11, 0, c, 1)]
        );
        assert_eq!(page_cursor(page.last_cursor()), tx(11, 0));
        assert!(page.has_more());
    }

    /// An `after` cursor's transaction is re-included by the widened wire bounds; the writes the
    /// cursor already covered are skipped.
    #[test]
    fn forward_after_tx_skips_covered_writes() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        let result = StreamPage::for_test(
            vec![pkg_item(10, &[(a, 1), (b, 1)]), pkg_item(11, &[(c, 1)])],
            None,
            None,
            Some(proto::QueryEndReason::LedgerTip),
        );

        let page = paginate(5, true, Some(tx(10, 0)), None, &result);

        assert_eq!(served(&page), vec![(10, 1, b, 1), (11, 0, c, 1)]);
    }

    /// Descending scans reverse writes within each transaction; the page's cursors describe scan
    /// sides, not ascending order.
    #[test]
    fn backward_scan_reverses_writes() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        // Descending scan order: tx 11 first, then tx 10.
        let result = StreamPage::for_test(
            vec![pkg_item(11, &[(b, 1), (c, 1)]), pkg_item(10, &[(a, 1)])],
            None,
            None,
            Some(proto::QueryEndReason::LedgerTip),
        );

        let page = paginate(5, false, None, None, &result);

        assert_eq!(
            served(&page),
            vec![(11, 1, c, 1), (11, 0, b, 1), (10, 0, a, 1)]
        );
        assert_eq!(page_cursor(page.first_cursor()), tx(11, 1));
        assert_eq!(page_cursor(page.last_cursor()), tx(10, 0));
    }

    /// A `before` cursor's transaction is re-included; the writes it already covered are skipped.
    #[test]
    fn backward_before_tx_skips_covered_writes() {
        let (a, b, c) = (pkg(1), pkg(2), pkg(3));
        let result = StreamPage::for_test(
            vec![pkg_item(11, &[(b, 1), (c, 1)]), pkg_item(10, &[(a, 1)])],
            None,
            None,
            Some(proto::QueryEndReason::LedgerTip),
        );

        let page = paginate(5, false, None, Some(tx(11, 1)), &result);

        assert_eq!(served(&page), vec![(11, 0, b, 1), (10, 0, a, 1)]);
    }

    /// A page that served nothing still reports scan progress through its watermark fences.
    #[test]
    fn empty_page_keeps_scan_fences() {
        let result: StreamPage<proto::ExecutedTransaction> = StreamPage::for_test(
            vec![],
            Some(wire_cursor(6, 8)),
            Some(wire_cursor(9, 15)),
            None,
        );

        let page = paginate(5, true, None, None, &result);

        assert!(page.items.is_empty());
        assert_eq!(page_cursor(page.first_cursor()), scan(6, 8));
        assert_eq!(page_cursor(page.last_cursor()), scan(9, 15));
        assert!(page.has_more());
    }

    #[test]
    fn scan_request_bounds_and_ordering() {
        let request = scan_request(&(3..=9), 5, true, None, None);
        assert_eq!(request.start_checkpoint, Some(3));
        // `cp_bounds` end is inclusive; the request bound is exclusive.
        assert_eq!(request.end_checkpoint, Some(10));

        let options = request.options.expect("options");
        // One extra transaction beyond the limit for a re-included boundary transaction.
        assert_eq!(options.limit, Some(6));
        assert_eq!(options.ordering, Some(proto::Ordering::Ascending as i32));
        assert_eq!(options.after, None);
        assert_eq!(options.before, None);

        let request = scan_request(&(3..=9), 5, false, None, None);
        let options = request.options.expect("options");
        assert_eq!(options.ordering, Some(proto::Ordering::Descending as i32));
    }

    /// A `Scan` after-bound excludes its transaction directly, hint intact.
    #[test]
    fn scan_request_after_scan_bounds_directly() {
        let request = scan_request(&(0..=9), 5, true, Some(&scan(4, 17)), None);
        let after = request.options.expect("options").after.expect("after");
        assert_eq!(wire_position(&after), (4, 17));
    }

    /// A `Tx` after-bound widens to re-include its transaction, hint reset to unknown.
    #[test]
    fn scan_request_after_tx_widens() {
        let request = scan_request(&(0..=9), 5, true, Some(&tx(17, 2)), None);
        let after = request.options.expect("options").after.expect("after");
        assert_eq!(wire_position(&after), (0, 16));
    }

    /// Nothing precedes the first transaction: a `Tx` after-bound at `tx_seq` 0 resumes from the
    /// range start.
    #[test]
    fn scan_request_after_tx_zero_unbounded() {
        let request = scan_request(&(0..=9), 5, true, Some(&tx(0, 2)), None);
        assert_eq!(request.options.expect("options").after, None);
    }

    /// `Scan` and first-write `Tx` before-bounds exclude their transaction directly, hint intact;
    /// a 0 hint is treated as unknown.
    #[test]
    fn scan_request_before_bounds_directly() {
        let request = scan_request(&(0..=9), 5, true, None, Some(&scan(4, 17)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (4, 17));

        let request = scan_request(&(0..=9), 5, true, None, Some(&tx(17, 0)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (CP, 17));

        let request = scan_request(&(0..=9), 5, true, None, Some(&scan(0, 17)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (u64::MAX, 17));
    }

    /// A mid-transaction `Tx` before-bound widens to re-include its transaction.
    #[test]
    fn scan_request_before_tx_widens() {
        let request = scan_request(&(0..=9), 5, true, None, Some(&tx(17, 3)));
        let before = request.options.expect("options").before.expect("before");
        assert_eq!(wire_position(&before), (u64::MAX, 18));
    }
}
