// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;

use bytes::Bytes;
use fastcrypto::hash::HashFunction;
use prost::Message;
use sui_inverted_index::ScanDirection;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::crypto::DefaultHash;

use sui_rpc::proto::sui::rpc::v2alpha::Ordering as ProtoOrdering;
use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2alpha::QueryOptions as ProtoQueryOptions;

const ORDERING_ASCENDING: i32 = ProtoOrdering::Ascending as i32;
const ORDERING_DESCENDING: i32 = ProtoOrdering::Descending as i32;
pub(crate) const MAX_CHECKPOINT_SCAN_WIDTH: u64 = 3_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Ordering {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum QueryType {
    Checkpoints,
    Transactions,
    Events,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
enum CursorKind {
    Item,
    Boundary,
}

/// Validated, normalized form of `QueryOptions` (the proto wire type).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueryOptions {
    query_type: QueryType,
    pub(crate) limit_items: usize,
    pub(crate) ordering: Ordering,
    after: Option<CursorToken>,
    before: Option<CursorToken>,
    scope_digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedCheckpointRange {
    pub(crate) range: Range<u64>,
    pub(crate) end_reason: QueryEndReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedRange {
    pub(crate) range: Range<u64>,
    pub(crate) end_checkpoint: u64,
    pub(crate) end_position: u64,
    pub(crate) end_reason: QueryEndReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CheckpointRange {
    start: u64,
    end: u64,
    high_reason: QueryEndReason,
    indexed_tip: u64,
}

impl QueryOptions {
    pub(crate) fn from_proto(
        request: Option<&ProtoQueryOptions>,
        default_limit_items: u32,
        max_limit_items: u32,
        query_type: QueryType,
        filter: Option<&impl Message>,
    ) -> Result<Self, RpcError> {
        let limit_items = request
            .and_then(|options| options.limit_items)
            .unwrap_or(default_limit_items)
            .clamp(1, max_limit_items) as usize;

        let ordering = match request.map(|options| options.ordering) {
            None | Some(ORDERING_ASCENDING) => Ordering::Ascending,
            Some(ORDERING_DESCENDING) => Ordering::Descending,
            Some(_) => {
                return Err(FieldViolation::new("options.ordering")
                    .with_description("invalid ordering")
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
        };

        let scope_digest = scope_digest(filter);
        let after = parse_cursor(
            "options.after",
            request.and_then(|options| options.after.as_ref()),
            query_type,
            scope_digest,
        )?;
        let before = parse_cursor(
            "options.before",
            request.and_then(|options| options.before.as_ref()),
            query_type,
            scope_digest,
        )?;

        Ok(Self {
            query_type,
            limit_items,
            ordering,
            after,
            before,
            scope_digest,
        })
    }

    pub(crate) fn scan_direction(&self) -> ScanDirection {
        match self.ordering {
            Ordering::Ascending => ScanDirection::Ascending,
            Ordering::Descending => ScanDirection::Descending,
        }
    }

    pub(crate) fn is_ascending(&self) -> bool {
        matches!(self.ordering, Ordering::Ascending)
    }

    pub(crate) fn apply_cursor_bounds(&self, resolved: ResolvedRange) -> ResolvedRange {
        if resolved.is_empty() {
            return resolved;
        }

        let mut start = resolved.range.start;
        let mut end = resolved.range.end;
        let mut end_checkpoint = resolved.end_checkpoint;
        let mut end_position = resolved.end_position;
        let mut end_reason = resolved.end_reason;
        let mut cursor_terminal = None;

        if let Some(cursor) = &self.after {
            let Some(after) = cursor.after_position_start() else {
                return ResolvedRange::empty_at(
                    cursor.checkpoint,
                    cursor.position,
                    QueryEndReason::CursorBound,
                );
            };
            if after >= start {
                start = after;
                if matches!(self.ordering, Ordering::Descending) || after >= end {
                    cursor_terminal = Some((cursor.checkpoint, after));
                }
                if matches!(self.ordering, Ordering::Descending) {
                    end_checkpoint = cursor.checkpoint;
                    end_position = after;
                    end_reason = QueryEndReason::CursorBound;
                }
            }
        }

        if let Some(cursor) = &self.before
            && cursor.position <= end
        {
            end = cursor.position;
            if matches!(self.ordering, Ordering::Ascending) || cursor.position <= start {
                cursor_terminal = Some((cursor.checkpoint, cursor.position));
            }
            if matches!(self.ordering, Ordering::Ascending) {
                end_checkpoint = cursor.checkpoint;
                end_position = cursor.position;
                end_reason = QueryEndReason::CursorBound;
            }
        }

        if start >= end {
            if let Some((checkpoint, position)) = cursor_terminal {
                end_checkpoint = checkpoint;
                end_position = position;
            }
            if self.after.is_some() || self.before.is_some() {
                end_reason = QueryEndReason::CursorBound;
            }
            ResolvedRange::empty_at(end_checkpoint, end_position, end_reason)
        } else {
            ResolvedRange {
                range: start..end,
                end_checkpoint,
                end_position,
                end_reason,
            }
        }
    }

    pub(crate) fn cursor_for_item(&self, checkpoint: u64, position: u64) -> Bytes {
        self.encode_cursor(CursorKind::Item, checkpoint, position)
    }

    pub(crate) fn cursor_for_boundary(&self, checkpoint: u64, position: u64) -> Bytes {
        self.encode_cursor(CursorKind::Boundary, checkpoint, position)
    }

    fn encode_cursor(&self, kind: CursorKind, checkpoint: u64, position: u64) -> Bytes {
        encode_cursor(CursorToken {
            query_type: self.query_type,
            kind,
            checkpoint,
            position,
            scope_digest: self.scope_digest,
        })
    }
}

impl ResolvedCheckpointRange {
    pub(crate) fn empty_at(checkpoint: u64, reason: QueryEndReason) -> Self {
        Self {
            range: checkpoint..checkpoint,
            end_reason: reason,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    pub(crate) fn terminal_checkpoint(&self, ordering: Ordering) -> u64 {
        match ordering {
            Ordering::Ascending => self.range.end,
            Ordering::Descending => self.range.start,
        }
    }

    pub(crate) fn with_range(self, range: Range<u64>, ordering: Ordering) -> ResolvedRange {
        let end_position = match ordering {
            Ordering::Ascending => range.end,
            Ordering::Descending => range.start,
        };
        ResolvedRange {
            range,
            end_checkpoint: self.terminal_checkpoint(ordering),
            end_position,
            end_reason: self.end_reason,
        }
    }
}

impl ResolvedRange {
    pub(crate) fn empty_at(
        end_checkpoint: u64,
        end_position: u64,
        end_reason: QueryEndReason,
    ) -> Self {
        Self {
            range: end_position..end_position,
            end_checkpoint,
            end_position,
            end_reason,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.range.is_empty()
    }

    pub(crate) fn end_cursor(&self, options: &QueryOptions) -> Bytes {
        options.cursor_for_boundary(self.end_checkpoint, self.end_position)
    }
}

impl CheckpointRange {
    pub(crate) fn from_request(
        start_checkpoint: Option<u64>,
        end_checkpoint: Option<u64>,
        checkpoint_hi_exclusive: u64,
    ) -> Result<Self, RpcError> {
        let start = start_checkpoint.unwrap_or(0);
        if let Some(end) = end_checkpoint
            && end < start
        {
            return Err(FieldViolation::new("end_checkpoint")
                .with_description(
                    "end_checkpoint must be greater than or equal to start_checkpoint",
                )
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

        let requested_end = end_checkpoint.unwrap_or(checkpoint_hi_exclusive);
        let high_reason = if end_checkpoint.is_none() || requested_end > checkpoint_hi_exclusive {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        };
        let end = requested_end.min(checkpoint_hi_exclusive);

        Ok(Self {
            start,
            end,
            high_reason,
            indexed_tip: checkpoint_hi_exclusive,
        })
    }

    pub(crate) fn resolve(self, options: &QueryOptions) -> ResolvedCheckpointRange {
        let mut start = self.start;
        let mut end = self.end;
        let mut low_reason = QueryEndReason::CheckpointBound;
        let mut high_reason = self.high_reason;
        let mut cursor_bound = false;

        if let Some(cursor) = &options.after
            && cursor.checkpoint >= start
        {
            start = cursor.checkpoint;
            cursor_bound = true;
            if matches!(options.ordering, Ordering::Descending) {
                low_reason = QueryEndReason::CursorBound;
            }
        }

        if let Some(cursor) = &options.before
            && let Some(upper) = cursor.before_checkpoint_end()
            && upper <= end
        {
            end = upper;
            cursor_bound = true;
            if matches!(options.ordering, Ordering::Ascending) {
                high_reason = QueryEndReason::CursorBound;
            }
        }

        if start >= self.indexed_tip {
            return ResolvedCheckpointRange::empty_at(self.indexed_tip, QueryEndReason::LedgerTip);
        }

        if start >= end {
            let reason = if cursor_bound {
                QueryEndReason::CursorBound
            } else {
                match options.ordering {
                    Ordering::Ascending => high_reason,
                    Ordering::Descending => low_reason,
                }
            };
            let checkpoint = match options.ordering {
                Ordering::Ascending => end,
                Ordering::Descending => start,
            };
            return ResolvedCheckpointRange::empty_at(checkpoint, reason);
        }

        match options.ordering {
            Ordering::Ascending => {
                let scan_end = end.min(start.saturating_add(MAX_CHECKPOINT_SCAN_WIDTH));
                let reason = if scan_end < end {
                    QueryEndReason::ScanLimit
                } else {
                    high_reason
                };
                ResolvedCheckpointRange {
                    range: start..scan_end,
                    end_reason: reason,
                }
            }
            Ordering::Descending => {
                let scan_start = start.max(end.saturating_sub(MAX_CHECKPOINT_SCAN_WIDTH));
                let reason = if scan_start > start {
                    QueryEndReason::ScanLimit
                } else {
                    low_reason
                };
                ResolvedCheckpointRange {
                    range: scan_start..end,
                    end_reason: reason,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct CursorToken {
    query_type: QueryType,
    kind: CursorKind,
    checkpoint: u64,
    position: u64,
    scope_digest: [u8; 32],
}

impl CursorToken {
    fn validate(&self, query_type: QueryType, scope_digest: [u8; 32]) -> bool {
        self.query_type == query_type && self.scope_digest == scope_digest
    }

    fn after_position_start(&self) -> Option<u64> {
        match self.kind {
            CursorKind::Item => self.position.checked_add(1),
            CursorKind::Boundary => Some(self.position),
        }
    }

    fn before_checkpoint_end(&self) -> Option<u64> {
        match self.kind {
            CursorKind::Item => self.checkpoint.checked_add(1),
            CursorKind::Boundary => Some(self.checkpoint),
        }
    }
}

fn parse_cursor(
    field: &'static str,
    cursor: Option<&Bytes>,
    query_type: QueryType,
    scope_digest: [u8; 32],
) -> Result<Option<CursorToken>, RpcError> {
    cursor
        .map(|cursor| decode_cursor(field, cursor))
        .transpose()?
        .map(|token| {
            if token.validate(query_type, scope_digest) {
                Ok(token)
            } else {
                Err(invalid_cursor(field, "invalid cursor"))
            }
        })
        .transpose()
}

fn decode_cursor(field: &'static str, cursor: &[u8]) -> Result<CursorToken, RpcError> {
    bcs::from_bytes(cursor).map_err(|_| invalid_cursor(field, "invalid cursor"))
}

fn encode_cursor(cursor: CursorToken) -> Bytes {
    bcs::to_bytes(&cursor).unwrap().into()
}

// This digest is a cursor-scope guard, not a portable canonical protobuf hash.
// We intentionally hash the server-side prost value after tonic/prost has
// decoded the request and dropped unknown fields, then re-encode it with our
// generated serializer. Protobuf wire bytes are not canonical, so do not
// replace this with hashing raw request bytes. If cursors need to remain
// compatible across proto/codegen/schema changes, replace this with a versioned
// canonical digest over the validated internal query representation.
fn scope_digest<F: Message>(filter: Option<&F>) -> [u8; 32] {
    let mut hasher = DefaultHash::default();
    hash_optional_message(&mut hasher, filter);
    hasher.finalize().digest
}

fn hash_optional_message<M: Message>(hasher: &mut DefaultHash, message: Option<&M>) {
    match message {
        None => hasher.update([0]),
        Some(message) => {
            hasher.update([1]);
            let bytes = message.encode_to_vec();
            let len = u32::try_from(bytes.len()).expect("scan scope part should fit in u32");
            hasher.update(len.to_be_bytes());
            hasher.update(bytes);
        }
    }
}

fn invalid_cursor(field: &'static str, description: impl Into<String>) -> RpcError {
    FieldViolation::new(field)
        .with_description(description)
        .with_reason(ErrorReason::FieldInvalid)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope_digest_for_filter() -> [u8; 32] {
        scope_digest(Option::<&ProtoQueryOptions>::None)
    }

    fn query_options_from_proto(
        request: Option<&ProtoQueryOptions>,
    ) -> Result<QueryOptions, RpcError> {
        QueryOptions::from_proto(
            request,
            100,
            1_000,
            QueryType::Transactions,
            Option::<&ProtoQueryOptions>::None,
        )
    }

    fn cursor_token(
        kind: CursorKind,
        checkpoint: u64,
        position: u64,
        query_type: QueryType,
    ) -> CursorToken {
        CursorToken {
            query_type,
            kind,
            checkpoint,
            position,
            scope_digest: scope_digest_for_filter(),
        }
    }

    fn item_cursor(checkpoint: u64, position: u64, query_type: QueryType) -> CursorToken {
        cursor_token(kind::ITEM, checkpoint, position, query_type)
    }

    fn boundary_cursor(checkpoint: u64, position: u64, query_type: QueryType) -> CursorToken {
        cursor_token(kind::BOUNDARY, checkpoint, position, query_type)
    }

    fn resolved_range(range: Range<u64>) -> ResolvedRange {
        ResolvedRange {
            range,
            end_checkpoint: 20,
            end_position: 20,
            end_reason: QueryEndReason::CheckpointBound,
        }
    }

    mod kind {
        use super::CursorKind;

        pub(super) const BOUNDARY: CursorKind = CursorKind::Boundary;
        pub(super) const ITEM: CursorKind = CursorKind::Item;
    }

    #[test]
    fn parses_cursors_and_ordering() {
        let after = encode_cursor(item_cursor(2, 20, QueryType::Transactions));
        let before = encode_cursor(item_cursor(3, 30, QueryType::Transactions));
        let mut request = ProtoQueryOptions::default();
        request.limit_items = Some(500);
        request.after = Some(after);
        request.before = Some(before);
        request.ordering = ProtoOrdering::Descending as i32;

        let options = query_options_from_proto(Some(&request)).unwrap();

        assert_eq!(options.limit_items, 500);
        assert_eq!(options.ordering, Ordering::Descending);
        assert_eq!(options.scan_direction(), ScanDirection::Descending);
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(0..100)).range,
            21..30
        );
    }

    #[test]
    fn clamps_limit_items_and_defaults_to_ascending() {
        let mut request = ProtoQueryOptions::default();
        request.limit_items = Some(5_000);

        let options = query_options_from_proto(Some(&request)).unwrap();

        assert_eq!(options.limit_items, 1_000);
        assert_eq!(options.ordering, Ordering::Ascending);
        assert_eq!(options.scan_direction(), ScanDirection::Ascending);
    }

    #[test]
    fn rejects_malformed_cursors_and_unknown_ordering() {
        let mut request = ProtoQueryOptions::default();
        request.after = Some(Bytes::from_static(b"short"));
        assert!(query_options_from_proto(Some(&request)).is_err());

        let mut request = ProtoQueryOptions::default();
        request.before = Some(Bytes::from_static(b"short"));
        assert!(query_options_from_proto(Some(&request)).is_err());

        let mut request = ProtoQueryOptions::default();
        request.ordering = 99;
        assert!(query_options_from_proto(Some(&request)).is_err());
    }

    #[test]
    fn rejects_cursor_for_different_query_type_or_scan_scope() {
        let token = encode_cursor(item_cursor(1, 9, QueryType::Checkpoints));
        let mut request = ProtoQueryOptions::default();
        request.after = Some(token);
        assert!(query_options_from_proto(Some(&request)).is_err());

        let token = encode_cursor(CursorToken {
            query_type: QueryType::Transactions,
            kind: CursorKind::Item,
            checkpoint: 1,
            position: 9,
            scope_digest: [9; 32],
        });
        let mut request = ProtoQueryOptions::default();
        request.before = Some(token);
        assert!(query_options_from_proto(Some(&request)).is_err());
    }

    #[test]
    fn accepts_cursors_for_different_checkpoint_range_and_ordering() {
        let token = encode_cursor(item_cursor(9, 9, QueryType::Transactions));
        let mut request = ProtoQueryOptions::default();
        request.after = Some(token);
        request.ordering = ProtoOrdering::Descending as i32;

        let options = query_options_from_proto(Some(&request)).unwrap();
        let range = CheckpointRange::from_request(Some(1_000), Some(1_100), 2_000).unwrap();

        assert_eq!(range.resolve(&options).range, 1_000..1_100);
    }

    #[test]
    fn applies_canonical_cursor_bounds() {
        let options = QueryOptions {
            query_type: QueryType::Transactions,
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: Some(item_cursor(1, 11, QueryType::Transactions)),
            before: None,
            scope_digest: scope_digest_for_filter(),
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            12..20
        );

        let options = QueryOptions {
            after: Some(item_cursor(1, u64::MAX, QueryType::Transactions)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)),
            ResolvedRange::empty_at(1, u64::MAX, QueryEndReason::CursorBound)
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: Some(item_cursor(1, 11, QueryType::Transactions)),
            before: Some(item_cursor(1, 19, QueryType::Transactions)),
            ..options
        };
        let bounded = options.apply_cursor_bounds(resolved_range(10..20));
        assert_eq!(bounded.range, 12..19);
        assert_eq!(bounded.end_reason, QueryEndReason::CursorBound);
        assert_eq!(bounded.end_position, 12);

        let options = QueryOptions {
            before: Some(item_cursor(1, 12, QueryType::Transactions)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)),
            ResolvedRange::empty_at(1, 12, QueryEndReason::CursorBound)
        );
    }

    #[test]
    fn applies_boundary_cursor_bounds_without_item_offset() {
        let options = QueryOptions {
            query_type: QueryType::Transactions,
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: Some(boundary_cursor(2, 20, QueryType::Transactions)),
            before: None,
            scope_digest: scope_digest_for_filter(),
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..30)).range,
            20..30
        );

        let options = QueryOptions {
            ordering: Ordering::Descending,
            after: None,
            before: Some(boundary_cursor(2, 20, QueryType::Transactions)),
            ..options
        };
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..30)).range,
            10..20
        );
    }

    #[test]
    fn resolves_checkpoint_range_with_terminal_reason() {
        assert_eq!(
            CheckpointRange::from_request(None, None, 20)
                .unwrap()
                .resolve(&query_options_from_proto(None).unwrap())
                .end_reason,
            QueryEndReason::LedgerTip
        );
        assert!(CheckpointRange::from_request(Some(10), Some(9), 20).is_err());
        assert!(
            CheckpointRange::from_request(Some(0), Some(MAX_CHECKPOINT_SCAN_WIDTH + 1), 20).is_ok()
        );

        let range =
            CheckpointRange::from_request(Some(10), Some(10 + MAX_CHECKPOINT_SCAN_WIDTH), 20)
                .unwrap();
        let resolved = range.resolve(&query_options_from_proto(None).unwrap());
        assert_eq!(resolved.range, 10..20);
        assert_eq!(resolved.end_reason, QueryEndReason::LedgerTip);

        let range = CheckpointRange::from_request(Some(10), None, 20).unwrap();
        let resolved = range.resolve(&query_options_from_proto(None).unwrap());
        assert_eq!(resolved.range, 10..20);
        assert_eq!(resolved.end_reason, QueryEndReason::LedgerTip);

        let range = CheckpointRange::from_request(Some(30), None, 20).unwrap();
        assert_eq!(
            range.resolve(&query_options_from_proto(None).unwrap()),
            ResolvedCheckpointRange::empty_at(20, QueryEndReason::LedgerTip)
        );
    }

    #[test]
    fn resolves_checkpoint_scan_window() {
        let options = query_options_from_proto(None).unwrap();
        let range = CheckpointRange::from_request(
            Some(10),
            Some(10 + MAX_CHECKPOINT_SCAN_WIDTH + 5),
            10 + MAX_CHECKPOINT_SCAN_WIDTH + 5,
        )
        .unwrap();
        let resolved = range.resolve(&options);
        assert_eq!(resolved.range, 10..10 + MAX_CHECKPOINT_SCAN_WIDTH);
        assert_eq!(resolved.end_reason, QueryEndReason::ScanLimit);

        let mut request = ProtoQueryOptions::default();
        request.ordering = ProtoOrdering::Descending as i32;
        let options = query_options_from_proto(Some(&request)).unwrap();
        let resolved = range.resolve(&options);
        assert_eq!(resolved.range, 15..10 + MAX_CHECKPOINT_SCAN_WIDTH + 5);
        assert_eq!(resolved.end_reason, QueryEndReason::ScanLimit);
    }

    #[test]
    fn item_cursor_can_be_used_as_after_or_before() {
        let options = QueryOptions {
            query_type: QueryType::Transactions,
            limit_items: 2,
            ordering: Ordering::Ascending,
            after: None,
            before: None,
            scope_digest: scope_digest_for_filter(),
        };
        let token = options.cursor_for_item(1, 11);

        let mut request = ProtoQueryOptions::default();
        request.after = Some(token.clone());
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            12..20
        );

        request.after = None;
        request.before = Some(token);
        let options = query_options_from_proto(Some(&request)).unwrap();
        assert_eq!(
            options.apply_cursor_bounds(resolved_range(10..20)).range,
            10..11
        );
    }
}
