// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;
use std::sync::Arc;

use anyhow::Context as _;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use async_graphql::connection::CursorType;
use async_graphql::connection::Edge;
use async_graphql::connection::EmptyFields;
use async_graphql::connection::PageInfo;
use prost_types::FieldMask;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2;
use sui_rpc_cursor::CursorKind;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::digests::CheckpointDigest;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointCommitment;
use sui_types::messages_checkpoint::CheckpointContents as NativeCheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;

use crate::api::query::Query;
use crate::api::scalars::base64::Base64;
use crate::api::scalars::cursor::ByteCursor;
use crate::api::scalars::cursor::JsonCursor;
use crate::api::scalars::cursor::MultiCursor;
use crate::api::scalars::cursor::OpaqueCursor;
use crate::api::scalars::date_time::DateTime;
use crate::api::scalars::id::Id;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::available_range::AvailableRangeKey;
use crate::api::types::checkpoint::filter::CheckpointFilter;
use crate::api::types::checkpoint::filter::checkpoint_bounds;
use crate::api::types::checkpoint::filter::cp_by_epoch;
use crate::api::types::checkpoint::filter::cp_unfiltered;
use crate::api::types::epoch::Epoch;
use crate::api::types::gas::GasCostSummary;
use crate::api::types::transaction::CTransaction;
use crate::api::types::transaction::Transaction;
use crate::api::types::transaction::TransactionConnection;
use crate::api::types::transaction::filter::TransactionFilter;
use crate::api::types::transaction::filter::TransactionFilterValidator as TFValidator;
use crate::api::types::validator_aggregated_signature::ValidatorAggregatedSignature;
use crate::error::RpcError;
use crate::error::upcast;
use crate::extensions::query_limits;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;
use crate::task::streaming::ProcessedCheckpoint;
use crate::task::watermark::Watermarks;

pub(crate) mod filter;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Cannot specify both `sequenceNumber` and `digest` on `Query.checkpoint`")]
    BothBoundsSet,
}

pub(crate) struct Checkpoint {
    pub(crate) sequence_number: u64,
    pub(crate) scope: Scope,
    /// Pre-processed data from streaming. When set, checkpoint fields are resolved from
    /// this data instead of fetching from the database.
    pub(crate) streamed_data: Option<Arc<ProcessedCheckpoint>>,
}

#[derive(Clone)]
struct CheckpointContents {
    scope: Scope,
    contents: Option<(
        CheckpointSummary,
        NativeCheckpointContents,
        AuthorityStrongQuorumSignInfo,
    )>,
    /// When set, transactions are resolved from this streamed data instead of the database.
    streamed_data: Option<Arc<ProcessedCheckpoint>>,
}

/// Validated checkpoint cursor coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointToken {
    /// Tracks the originating `CursorToken`'s kind, so it can be reproduced on re-encode.
    kind: CursorKind,
    checkpoint: u64,
}

/// Custom `Connection` for checkpoints to support partially-filled or empty pages that can carry
/// resume cursors.
pub(crate) struct CheckpointConnection {
    pub edges: Vec<Edge<String, Checkpoint, EmptyFields>>,
    pub page_info: PageInfo,
}

/// Compatibility dispatch over the on-wire cursor formats: `CursorToken` (primary) or the
/// legacy JSON cursor (secondary).
pub type CCheckpoint = MultiCursor<OpaqueCursor<CheckpointToken>, JsonCursor<u64>>;

/// Checkpoints contain finalized transactions and are used for node synchronization and global transaction ordering.
#[Object]
impl Checkpoint {
    /// The checkpoint's globally unique identifier, which can be passed to `Query.node` to refetch it.
    pub(crate) async fn id(&self) -> Id {
        Id::Checkpoint(self.sequence_number)
    }

    /// The checkpoint's position in the total order of finalized checkpoints, agreed upon by consensus.
    async fn sequence_number(&self) -> UInt53 {
        self.sequence_number.into()
    }

    /// Query the RPC as if this checkpoint were the latest checkpoint.
    async fn query(&self, ctx: &Context<'_>) -> Option<Result<Query, RpcError>> {
        async {
            let scope = Some(
                self.scope
                    .with_checkpoint_viewed_at(ctx, self.sequence_number)
                    .context("Checkpoint in the future")?,
            );

            Ok(Some(Query { scope }))
        }
        .await
        .transpose()
    }

    #[graphql(flatten)]
    async fn contents(&self, ctx: &Context<'_>) -> Result<CheckpointContents, RpcError> {
        if let Some(processed) = &self.streamed_data {
            CheckpointContents::from_streamed_checkpoint(self.scope.clone(), processed)
        } else {
            CheckpointContents::fetch(ctx, self.scope.clone(), self.sequence_number).await
        }
    }
}

#[Object]
impl CheckpointContents {
    /// A commitment by the committee at each checkpoint on the artifacts of the checkpoint.
    /// e.g., object checkpoint states
    async fn artifacts_digest(&self) -> Option<Result<String, RpcError>> {
        let (summary, _, _) = self.contents.as_ref()?;

        for commitment in &summary.checkpoint_commitments {
            if let CheckpointCommitment::CheckpointArtifactsDigest(digest) = commitment {
                return Some(Ok(digest.base58_encode()));
            }
        }

        None
    }

    /// A 32-byte hash that uniquely identifies the checkpoint, encoded in Base58. This is a hash of the checkpoint's summary.
    async fn digest(&self) -> Option<Result<String, RpcError>> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Ok(summary.digest().base58_encode()))
    }

    /// A 32-byte hash that uniquely identifies the checkpoint's content, encoded in Base58.
    async fn content_digest(&self) -> Option<Result<String, RpcError>> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Ok(summary.content_digest.base58_encode()))
    }

    /// The epoch that this checkpoint is part of.
    async fn epoch(&self) -> Option<Epoch> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Epoch::with_id(self.scope.clone(), summary.epoch))
    }

    /// The total number of transactions in the network by the end of this checkpoint.
    async fn network_total_transactions(&self) -> Option<UInt53> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(summary.network_total_transactions.into())
    }

    /// The digest of the previous checkpoint's summary.
    async fn previous_checkpoint_digest(&self) -> Option<Result<String, RpcError>> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(Ok(summary.previous_digest.as_ref()?.base58_encode()))
    }

    /// The computation cost, storage cost, storage rebate, and non-refundable storage fee accumulated during this epoch, up to and including this checkpoint. These values increase monotonically across checkpoints in the same epoch, and reset on epoch boundaries.
    async fn rolling_gas_summary(&self) -> Option<GasCostSummary> {
        let (summary, _, _) = self.contents.as_ref()?;
        Some(GasCostSummary::from(
            summary.epoch_rolling_gas_cost_summary.clone(),
        ))
    }

    /// The Base64 serialized BCS bytes of this checkpoint's summary.
    async fn summary_bcs(&self) -> Option<Result<Base64, RpcError>> {
        async {
            let Some((summary, _, _)) = &self.contents else {
                return Ok(None);
            };
            Ok(Some(Base64::from(
                bcs::to_bytes(summary).context("Failed to serialize checkpoint summary")?,
            )))
        }
        .await
        .transpose()
    }

    /// The Base64 serialized BCS bytes of this checkpoint's contents.
    async fn content_bcs(&self) -> Option<Result<Base64, RpcError>> {
        async {
            let Some((_, content, _)) = &self.contents else {
                return Ok(None);
            };
            Ok(Some(Base64::from(
                bcs::to_bytes(content).context("Failed to serialize checkpoint content")?,
            )))
        }
        .await
        .transpose()
    }

    /// The timestamp at which the checkpoint is agreed to have happened according to consensus. Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Option<Result<DateTime, RpcError>> {
        async {
            let Some((summary, _, _)) = &self.contents else {
                return Ok(None);
            };

            Ok(Some(DateTime::from_ms(summary.timestamp_ms as i64)?))
        }
        .await
        .transpose()
    }

    /// The aggregation of signatures from a quorum of validators for the checkpoint proposal.
    async fn validator_signatures(&self) -> Option<Result<ValidatorAggregatedSignature, RpcError>> {
        let (_, _, authority_info) = self.contents.as_ref()?;
        Some(Ok(ValidatorAggregatedSignature::with_authority_info(
            self.scope.clone(),
            authority_info.clone(),
        )))
    }

    // The transactions in this checkpoint.
    async fn transactions(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CTransaction>,
        last: Option<u64>,
        before: Option<CTransaction>,
        #[graphql(validator(custom = "TFValidator"))] filter: Option<TransactionFilter>,
    ) -> Option<Result<TransactionConnection, RpcError>> {
        async {
            let Some((summary, _, _)) = &self.contents else {
                return Ok(None);
            };

            let pagination: &PaginationConfig = ctx.data()?;
            let limits = pagination.limits("Checkpoint", "transactions");
            let page = Page::from_params(limits, first, after, last, before)?;

            let Some(filter) = filter.unwrap_or_default().intersect(TransactionFilter {
                at_checkpoint: Some(UInt53::from(summary.sequence_number)),
                ..Default::default()
            }) else {
                return Ok(Some(Connection::new(false, false).into()));
            };

            if let Some(streamed) = &self.streamed_data {
                return Ok(Some(Transaction::paginate_preloaded_transactions(
                    self.scope.clone(),
                    summary.sequence_number,
                    &streamed.transactions,
                    &page,
                    filter,
                )?));
            }

            Ok(Some(
                Transaction::paginate(ctx, self.scope.clone(), page, filter)
                    .await
                    .map_err(upcast)?,
            ))
        }
        .await
        .transpose()
    }
}

#[Object]
impl CheckpointConnection {
    /// Information to aid in pagination.
    async fn page_info(&self) -> &PageInfo {
        &self.page_info
    }

    /// A list of edges.
    async fn edges(&self) -> &[Edge<String, Checkpoint, EmptyFields>] {
        &self.edges
    }

    /// A list of nodes.
    async fn nodes(&self) -> Vec<&Checkpoint> {
        self.edges.iter().map(|e| &e.node).collect()
    }
}

impl Checkpoint {
    /// Construct a checkpoint that is represented by just its identifier (its sequence number).
    ///
    /// If no sequence_number is provided, defaults to the scope's checkpoint.
    /// Returns `None` if the checkpoint is set in the future relative to the current scope's
    /// checkpoint, or when no checkpoint is set in scope (e.g. execution scope, where checkpoint
    /// queries return None to prevent temporal inconsistency).
    pub(crate) fn with_sequence_number(scope: Scope, sequence_number: Option<u64>) -> Option<Self> {
        let scope_checkpoint = scope.checkpoint_viewed_at()?;
        let sequence_number = sequence_number.unwrap_or(scope_checkpoint);

        (sequence_number <= scope_checkpoint).then_some(Self {
            scope,
            sequence_number,
            streamed_data: None,
        })
    }

    /// Resolve a checkpoint by its digest. Translates the digest to a sequence number via the
    /// configured KV reader, then delegates to `with_sequence_number` so all downstream resolvers
    /// behave the same as the sequence-number path.
    pub(crate) async fn by_digest(
        ctx: &Context<'_>,
        scope: Scope,
        digest: CheckpointDigest,
    ) -> Result<Option<Self>, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;
        let Some(sequence_number) = kv_loader
            .load_one_checkpoint_seq_by_digest(digest)
            .await
            .context("Failed to look up checkpoint by digest")?
        else {
            return Ok(None);
        };

        Ok(Self::with_sequence_number(scope, Some(sequence_number)))
    }

    /// Paginate through checkpoints with filters applied.
    ///
    /// Returns empty results when no checkpoint is set in scope (e.g. execution scope).
    pub(crate) async fn paginate(
        ctx: &Context<'_>,
        scope: Scope,
        page: Page<CCheckpoint>,
        filter: CheckpointFilter,
    ) -> Result<CheckpointConnection, RpcError> {
        if let Some(reader) = ctx.data_opt::<AlphaLedgerGrpcReader>() {
            return Self::paginate_grpc(ctx, reader, scope, page, filter).await;
        }

        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let available_range_key = AvailableRangeKey {
            type_: "Query".to_string(),
            field: Some("checkpoints".to_string()),
            filters: Some(filter.active_filters()),
        };
        let reader_lo = available_range_key.reader_lo(watermarks)?;

        let Some(cp_hi_inclusive) = scope.checkpoint_viewed_at() else {
            // In execution scope, checkpoint pagination returns empty results
            return Ok(CheckpointConnection::empty());
        };

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            cp_hi_inclusive,
        ) else {
            return Ok(CheckpointConnection::empty());
        };

        let results = if let Some(epoch) = filter.at_epoch {
            cp_by_epoch(ctx, &page, &cp_bounds, epoch.into()).await?
        } else {
            cp_unfiltered(&cp_bounds, &page)
        };

        page.paginate_results(
            results,
            |c| CheckpointToken::cursor(*c),
            |c| Ok(Self::with_sequence_number(scope.clone(), Some(c)).unwrap()),
        )
        .map(Into::into)
    }

    /// Serve checkpoint pagination by streaming gRPC. Returns pages that may be partially filled,
    /// with valid cursors if there are more pages to paginate through.
    async fn paginate_grpc(
        ctx: &Context<'_>,
        reader: &AlphaLedgerGrpcReader,
        scope: Scope,
        page: Page<CCheckpoint>,
        filter: CheckpointFilter,
    ) -> Result<CheckpointConnection, RpcError> {
        query_limits::rich::debit(ctx)?;

        if page.limit() == 0 {
            return Ok(CheckpointConnection::empty());
        }

        // Consistency upper bound; empty when scope has no checkpoint set.
        let Some(checkpoint_viewed_at) = scope.checkpoint_viewed_at() else {
            return Ok(CheckpointConnection::empty());
        };

        // TODO: LedgerService expose available checkpoint range for `reader_lo`.
        let reader_lo = 0;

        let Some(cp_bounds) = checkpoint_bounds(
            filter.after_checkpoint.map(u64::from),
            filter.at_checkpoint.map(u64::from),
            filter.before_checkpoint.map(u64::from),
            reader_lo,
            checkpoint_viewed_at,
        ) else {
            return Ok(CheckpointConnection::empty());
        };

        // `atEpoch` has no dimension in the gRPC filter (a DNF over transaction predicates) —
        // resolve the epoch to its checkpoint range with a point-read and tighten the request's
        // bounds instead.
        let cp_bounds = if let Some(epoch) = filter.at_epoch {
            match epoch_cps(reader, epoch.into()).await? {
                Some(epoch_bounds) => {
                    intersect_epoch_bounds(epoch_bounds.0, epoch_bounds.1, &cp_bounds)
                        .context("Epoch's checkpoint range is disjoint from the requested bounds")?
                }
                None => return Ok(CheckpointConnection::empty()),
            }
        } else {
            cp_bounds
        };

        // Extract the cursor and pass through to grpc. The checkpoint sequence number is the whole
        // position, so legacy JSON cursors translate losslessly (unlike transactions/events, whose
        // legacy cursors lack a checkpoint hint).
        let after = page.after().map(|c| CursorToken::from(&c.token()).encode());
        let before = page
            .before()
            .map(|c| CursorToken::from(&c.token()).encode());

        let mut options = v2::QueryOptions::default();
        options.limit = Some(page.limit() as u32);
        options.after = after;
        options.before = before;
        options.ordering = Some(if page.is_from_front() {
            v2::Ordering::Ascending as i32
        } else {
            v2::Ordering::Descending as i32
        });

        let mut request = v2::ListCheckpointsRequest::default();
        // Sequence number only — checkpoint contents hydrate lazily via `KvLoader` on field
        // access.
        request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
        request.start_checkpoint = Some(*cp_bounds.start());
        // `cp_bounds` end is inclusive; the request bound is exclusive.
        request.end_checkpoint = Some(cp_bounds.end().saturating_add(1));
        request.options = Some(options);

        let result = reader
            .list_checkpoints(request)
            .await
            .context("Failed to list checkpoints")?;

        build_grpc_connection(scope, &page, result)
    }
}

impl CheckpointContents {
    /// Attempt to fill the contents. If the contents are already filled, returns a clone,
    /// otherwise attempts to fetch from the store. The resulting value may still have an empty
    /// contents field, because it could not be found in the store.
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        scope: Scope,
        sequence_number: u64,
    ) -> Result<Self, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;
        let contents = kv_loader
            .load_one_checkpoint(sequence_number)
            .await
            .context("Failed to fetch checkpoint contents")?;

        Ok(Self {
            scope,
            contents,
            streamed_data: None,
        })
    }

    /// Construct from pre-processed streamed checkpoint data.
    fn from_streamed_checkpoint(
        scope: Scope,
        processed: &Arc<ProcessedCheckpoint>,
    ) -> Result<Self, RpcError> {
        Ok(Self {
            scope,
            contents: Some((
                processed.summary.clone(),
                processed.contents.clone(),
                processed.signature.clone(),
            )),
            streamed_data: Some(Arc::clone(processed)),
        })
    }
}

impl CheckpointToken {
    /// Mint the edge cursor for the checkpoint at the given sequence number.
    pub fn cursor(checkpoint: u64) -> CCheckpoint {
        CCheckpoint::new(OpaqueCursor::new(Self {
            kind: CursorKind::Item,
            checkpoint,
        }))
    }
}

impl CCheckpoint {
    pub(crate) fn sequence_number(&self) -> u64 {
        match self {
            CCheckpoint::Primary(c) => c.checkpoint,
            CCheckpoint::Secondary(c) => **c,
        }
    }

    /// View the cursor as validated checkpoint coordinates, regardless of wire format.
    fn token(&self) -> CheckpointToken {
        match self {
            CCheckpoint::Primary(c) => (**c).clone(),
            CCheckpoint::Secondary(c) => CheckpointToken {
                kind: CursorKind::Item,
                checkpoint: **c,
            },
        }
    }
}

impl CheckpointConnection {
    pub(crate) fn empty() -> Self {
        Self {
            edges: vec![],
            page_info: PageInfo {
                has_previous_page: false,
                has_next_page: false,
                start_cursor: None,
                end_cursor: None,
            },
        }
    }
}

impl ByteCursor for CheckpointToken {
    fn decode_cursor(bytes: &[u8]) -> anyhow::Result<Self> {
        CursorToken::decode(bytes)?.try_into()
    }

    fn encode_cursor(&self) -> bytes::Bytes {
        CursorToken::from(self).encode()
    }
}

impl From<&CheckpointToken> for CursorToken {
    fn from(token: &CheckpointToken) -> Self {
        CursorToken {
            kind: token.kind,
            position: Position::Checkpoints {
                checkpoint: token.checkpoint,
            },
        }
    }
}

impl TryFrom<CursorToken> for CheckpointToken {
    type Error = anyhow::Error;

    fn try_from(token: CursorToken) -> anyhow::Result<Self> {
        let Position::Checkpoints { checkpoint } = token.position else {
            anyhow::bail!("invalid cursor");
        };
        Ok(Self {
            kind: token.kind,
            checkpoint,
        })
    }
}

impl Eq for CCheckpoint {}

/// Cursors minted by different paths can disagree on the kind, so pagination only compares the
/// checkpoint coordinate.
impl PartialEq for CCheckpoint {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number() == other.sequence_number()
    }
}

impl From<Connection<String, Checkpoint>> for CheckpointConnection {
    /// Convert a stock async-graphql `Connection` (as produced by the PG path's
    /// `Page::paginate_results`) into the custom shape. Cursors are derived from edges, matching
    /// stock semantics.
    fn from(conn: Connection<String, Checkpoint>) -> Self {
        let start_cursor = conn.edges.first().map(|e| e.cursor.clone());
        let end_cursor = conn.edges.last().map(|e| e.cursor.clone());
        Self {
            edges: conn.edges,
            page_info: PageInfo {
                has_previous_page: conn.has_previous_page,
                has_next_page: conn.has_next_page,
                start_cursor,
                end_cursor,
            },
        }
    }
}

/// Helper to extract the first and last checkpoint of an epoch.
async fn epoch_cps(
    reader: &AlphaLedgerGrpcReader,
    epoch: u64,
) -> Result<Option<(u64, Option<u64>)>, RpcError> {
    let mut request = v2::GetEpochRequest::default();
    request.epoch = Some(epoch);
    request.read_mask = Some(FieldMask::from_paths([
        "epoch",
        "first_checkpoint",
        "last_checkpoint",
    ]));

    let Some(epoch) = reader
        .get_epoch(request)
        .await
        .context("Failed to get epoch")?
    else {
        return Ok(None);
    };

    let first = epoch
        .first_checkpoint
        .context("GetEpoch response missing first checkpoint")?;

    Ok(Some((first, epoch.last_checkpoint)))
}

/// Intersect an epoch's checkpoint range (`first`, and `last` if the epoch has ended) with
/// `cp_bounds`. `None` when the intersection is empty.
fn intersect_epoch_bounds(
    first: u64,
    last: Option<u64>,
    cp_bounds: &RangeInclusive<u64>,
) -> Option<RangeInclusive<u64>> {
    let lo = first.max(*cp_bounds.start());
    let hi = last.map_or(*cp_bounds.end(), |last| last.min(*cp_bounds.end()));
    (lo <= hi).then(|| lo..=hi)
}

/// Build a `CheckpointConnection` from draining a bitmap-scan page.
///
/// Edges are returned in ascending order.
fn build_grpc_connection(
    scope: Scope,
    page: &Page<CCheckpoint>,
    result: StreamPage<v2::Checkpoint>,
) -> Result<CheckpointConnection, RpcError> {
    let more = result.has_more();
    let start = result.first_cursor().cloned();
    let end = result.last_cursor().cloned();
    let mut items = result.items;

    let (has_previous_page, has_next_page, start, end) = if page.is_from_front() {
        (page.after().is_some(), more, start, end)
    } else {
        items.reverse();
        (more, page.before().is_some(), end, start)
    };

    let mut edges = Vec::with_capacity(items.len());
    for item in items {
        let sequence_number = item
            .payload
            .sequence_number
            .context("ListCheckpoints item missing sequence number")?;

        // Constructed directly rather than through `with_sequence_number`: items are bounded by
        // the request's checkpoint range, which is itself capped at the scope's checkpoint.
        let checkpoint = Checkpoint {
            sequence_number,
            scope: scope.clone(),
            streamed_data: None,
        };

        edges.push(Edge::new(encode_grpc_cursor(&item.cursor)?, checkpoint));
    }

    let start_cursor = start.map(|b| encode_grpc_cursor(&b)).transpose()?;
    let end_cursor = end.map(|b| encode_grpc_cursor(&b)).transpose()?;

    Ok(CheckpointConnection {
        edges,
        page_info: PageInfo {
            has_previous_page,
            has_next_page,
            start_cursor,
            end_cursor,
        },
    })
}

/// Re-encode a server-minted cursor (raw encoded `CursorToken` bytes from the gRPC stream) as a
/// GraphQL cursor string.
fn encode_grpc_cursor(bytes: &[u8]) -> Result<String, RpcError> {
    let token = CursorToken::decode(bytes).context("Failed to decode ListCheckpoints cursor")?;
    let token: CheckpointToken = token
        .try_into()
        .context("Unexpected position in ListCheckpoints cursor")?;
    Ok(CCheckpoint::new(OpaqueCursor::new(token)).encode_cursor())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::connection::CursorType;
    use fastcrypto::encoding::Base64 as B64;
    use fastcrypto::encoding::Encoding;

    /// Legacy pg-style cursor: a bare JSON-encoded checkpoint sequence number.
    fn legacy_cursor(checkpoint: u64) -> CCheckpoint {
        CCheckpoint::Secondary(JsonCursor::new(checkpoint))
    }

    #[test]
    fn primary_cursor_roundtrips() {
        let cursor = CheckpointToken::cursor(42);
        let decoded = CCheckpoint::decode_cursor(&cursor.encode_cursor()).expect("valid cursor");
        assert_eq!(decoded.sequence_number(), 42);
        assert_eq!(decoded, cursor);
    }

    /// A legacy cursor paginates the same as a grpc cursor at the same sequence number.
    #[test]
    fn legacy_cursor_matches_primary() {
        assert_eq!(legacy_cursor(42).sequence_number(), 42);
        assert_eq!(legacy_cursor(42), CheckpointToken::cursor(42));
    }

    /// A token scoped to another endpoint must not decode as a checkpoint cursor.
    #[test]
    fn rejects_wrong_variant_cursor() {
        let token = CursorToken::item(Position::Transactions {
            checkpoint: 1,
            tx_seq: 2,
        });
        let encoded = B64::encode(token.encode());
        assert!(CCheckpoint::decode_cursor(&encoded).is_err());
    }

    /// Legacy JSON cursors carry the full position (the sequence number), so the coordinate view
    /// is lossless.
    #[test]
    fn legacy_cursor_token_coordinates() {
        let token = legacy_cursor(42).token();
        assert_eq!(token.kind, CursorKind::Item);
        assert_eq!(token.checkpoint, 42);
    }

    #[test]
    fn epoch_bounds_closed_epoch_intersects() {
        assert_eq!(
            intersect_epoch_bounds(10, Some(20), &(0..=100)),
            Some(10..=20)
        );
        // Bounds tighter than the epoch on both sides.
        assert_eq!(
            intersect_epoch_bounds(10, Some(20), &(12..=18)),
            Some(12..=18)
        );
    }

    /// An ongoing epoch has no last checkpoint; the upper bound stays the caller's (already capped
    /// at the scope's checkpoint).
    #[test]
    fn epoch_bounds_ongoing_epoch_clamps_to_caller_hi() {
        assert_eq!(intersect_epoch_bounds(10, None, &(0..=100)), Some(10..=100));
    }

    #[test]
    fn epoch_bounds_disjoint_is_none() {
        // Epoch entirely below the bounds.
        assert_eq!(intersect_epoch_bounds(0, Some(5), &(10..=100)), None);
        // Epoch entirely above the bounds.
        assert_eq!(intersect_epoch_bounds(200, Some(300), &(10..=100)), None);
        assert_eq!(intersect_epoch_bounds(200, None, &(10..=100)), None);
    }

    mod grpc_connection {
        use sui_indexer_alt_reader::alpha_ledger_grpc_reader::PageItem;

        use super::*;
        use crate::pagination::PageLimits;

        fn cp_position(checkpoint: u64) -> Position {
            Position::Checkpoints { checkpoint }
        }

        /// Build a synthetic `PageItem` for the checkpoint at `sequence_number`, with the provided
        /// resume cursor.
        fn cp_item(sequence_number: u64, cursor: CursorToken) -> PageItem<v2::Checkpoint> {
            let mut payload = v2::Checkpoint::default();
            payload.sequence_number = Some(sequence_number);
            PageItem {
                payload,
                cursor: cursor.encode(),
            }
        }

        /// The GraphQL cursor string that `build_grpc_connection` mints for raw server cursor
        /// bytes.
        fn graphql_cursor(token: CursorToken) -> String {
            let token: CheckpointToken = token.try_into().expect("checkpoints cursor");
            CCheckpoint::new(OpaqueCursor::new(token)).encode_cursor()
        }

        fn page_limits(limit: u64) -> PageLimits {
            PageLimits {
                default: limit as u32,
                max: limit as u32,
            }
        }

        /// Build a `Page<CCheckpoint>` going forwards (`first: N`, no `after`/`before`).
        fn forward_page(limit: u64) -> Page<CCheckpoint> {
            Page::from_params(&page_limits(limit), Some(limit), None, None, None)
                .expect("constructing forward Page<CCheckpoint>")
        }

        /// Build a `Page<CCheckpoint>` going backwards (`last: N`, no `after`/`before`).
        fn backward_page(limit: u64) -> Page<CCheckpoint> {
            Page::from_params(&page_limits(limit), None, None, Some(limit), None)
                .expect("constructing backward Page<CCheckpoint>")
        }

        /// Forward page opened from an `after` cursor (`first: N, after: <cursor>`).
        fn forward_page_after(limit: u64, after: CCheckpoint) -> Page<CCheckpoint> {
            Page::from_params(&page_limits(limit), Some(limit), Some(after), None, None)
                .expect("constructing forward Page with after")
        }

        /// Backward page opened from a `before` cursor (`last: N, before: <cursor>`).
        fn backward_page_before(limit: u64, before: CCheckpoint) -> Page<CCheckpoint> {
            Page::from_params(&page_limits(limit), None, None, Some(limit), Some(before))
                .expect("constructing backward Page with before")
        }

        /// Empty connection surfaces cursors if provided by the streamed page.
        #[test]
        fn empty_page_surfaces_boundary_cursors() {
            let scope = Scope::for_tests();
            let page = forward_page(10);
            let result = StreamPage::<v2::Checkpoint>::for_test(
                Vec::new(),
                Some(CursorToken::boundary(cp_position(10)).encode()),
                Some(CursorToken::boundary(cp_position(20)).encode()),
                None,
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(conn.edges.is_empty());
            assert!(!conn.page_info.has_previous_page);
            assert!(conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start cursor set");
            let end = conn.page_info.end_cursor.expect("end cursor set");
            assert_ne!(start, end, "start and end cursors should be different");
        }

        /// Order of cursors on connection should be swapped from streamed page.
        #[test]
        fn empty_page_backward_correct_cursors() {
            let scope = Scope::for_tests();
            let page = backward_page(10);
            // Descending stream: the first watermark the stream reports is the high end.
            let result = StreamPage::<v2::Checkpoint>::for_test(
                Vec::new(),
                Some(CursorToken::boundary(cp_position(20)).encode()),
                Some(CursorToken::boundary(cp_position(10)).encode()),
                None,
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(conn.edges.is_empty());
            assert!(conn.page_info.has_previous_page);
            assert!(!conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start cursor set");
            let end = conn.page_info.end_cursor.expect("end cursor set");
            assert_eq!(
                start,
                graphql_cursor(CursorToken::boundary(cp_position(10)))
            );
            assert_eq!(end, graphql_cursor(CursorToken::boundary(cp_position(20))));
        }

        #[test]
        fn non_empty_page_uses_edge_cursors() {
            let scope = Scope::for_tests();
            let page = forward_page(10);
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![
                    cp_item(1, CursorToken::item(cp_position(1))),
                    cp_item(2, CursorToken::item(cp_position(2))),
                    cp_item(3, CursorToken::item(cp_position(3))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 3);
            // `CheckpointBound` means the range was exhausted — no forward continuation.
            assert!(!conn.page_info.has_next_page);

            let start = conn.page_info.start_cursor.expect("start set");
            let end = conn.page_info.end_cursor.expect("end set");
            assert_eq!(
                start, conn.edges[0].cursor,
                "non-empty page should anchor start_cursor on first edge, not stream watermark"
            );
            assert_eq!(
                end, conn.edges[2].cursor,
                "non-empty page should anchor end_cursor on last edge, not stream watermark"
            );

            for (edge, sequence_number) in conn.edges.iter().zip([1u64, 2, 3]) {
                assert_eq!(edge.node.sequence_number, sequence_number);
            }
        }

        #[test]
        fn full_page_at_item_limit_signals_more() {
            let scope = Scope::for_tests();
            let page = forward_page(2);
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![
                    cp_item(1, CursorToken::item(cp_position(1))),
                    cp_item(2, CursorToken::item(cp_position(2))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::ItemLimit),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 2);
            assert!(
                conn.page_info.has_next_page,
                "full page + ItemLimit must report hasNextPage: true (has_more() is true)"
            );
        }

        #[test]
        fn descending_page_reverses_to_ascending_edges() {
            let scope = Scope::for_tests();
            let page = backward_page(10);
            // Descending stream order: checkpoints 3, 2, 1 (highest position first).
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![
                    cp_item(3, CursorToken::item(cp_position(3))),
                    cp_item(2, CursorToken::item(cp_position(2))),
                    cp_item(1, CursorToken::item(cp_position(1))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert_eq!(conn.edges.len(), 3);
            // After reversal, the *first* edge corresponds to the *lowest* position from the
            // stream — i.e. the last item the stream emitted (checkpoint 1).
            let start = conn.page_info.start_cursor.expect("start set");
            let end = conn.page_info.end_cursor.expect("end set");
            assert_eq!(start, conn.edges[0].cursor);
            assert_eq!(start, graphql_cursor(CursorToken::item(cp_position(1))));
            assert_eq!(end, conn.edges[2].cursor);
            assert_eq!(end, graphql_cursor(CursorToken::item(cp_position(3))));
            assert_eq!(
                conn.edges
                    .iter()
                    .map(|e| e.node.sequence_number)
                    .collect::<Vec<_>>(),
                [1, 2, 3],
            );
        }

        /// A forward page opened from an `after` cursor reports `hasPreviousPage: true`
        /// (`page.after().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
        /// source of a `true` flag is the input cursor — not the stream.
        #[test]
        fn forward_after_signals_previous_page() {
            let scope = Scope::for_tests();
            let page = forward_page_after(10, CheckpointToken::cursor(1));
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![cp_item(2, CursorToken::item(cp_position(2)))],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(
                conn.page_info.has_previous_page,
                "after cursor set → hasPreviousPage"
            );
            assert!(
                !conn.page_info.has_next_page,
                "CheckpointBound → no hasNextPage"
            );
        }

        /// A backward page opened from a `before` cursor reports `hasNextPage: true`
        /// (`page.before().is_some()`). `CheckpointBound` makes `has_more()` false, so the only
        /// source of a `true` flag is the input cursor — not the stream.
        #[test]
        fn backward_before_signals_next_page() {
            let scope = Scope::for_tests();
            let page = backward_page_before(10, CheckpointToken::cursor(3));
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![
                    cp_item(2, CursorToken::item(cp_position(2))),
                    cp_item(1, CursorToken::item(cp_position(1))),
                ],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );

            let conn = build_grpc_connection(scope, &page, result).expect("connection built");
            assert!(
                conn.page_info.has_next_page,
                "before cursor set → hasNextPage"
            );
            assert!(
                !conn.page_info.has_previous_page,
                "CheckpointBound → no hasPreviousPage"
            );
        }

        /// An item without a sequence number is an internal inconsistency, not an empty result.
        #[test]
        fn missing_sequence_number_errors() {
            let scope = Scope::for_tests();
            let page = forward_page(10);
            let result = StreamPage::<v2::Checkpoint>::for_test(
                vec![PageItem {
                    payload: v2::Checkpoint::default(),
                    cursor: CursorToken::item(cp_position(1)).encode(),
                }],
                None,
                None,
                Some(v2::QueryEndReason::CheckpointBound),
            );
            assert!(
                build_grpc_connection(scope, &page, result).is_err(),
                "missing sequence number should error"
            );
        }
    }
}
