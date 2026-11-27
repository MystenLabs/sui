// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Cursor encoding/decoding, legacy BCS page-token fallback, and `QueryOptions` parsing for
//! `StateService` listings.

use std::ops::Bound;

use bytes::Bytes;
use move_core_types::language_storage::StructTag;
use prost::Message as _;
use serde::de::DeserializeOwned;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::Ordering as ProtoOrdering;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions as ProtoQueryOptions;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SUI_ADDRESS_LENGTH;
use sui_types::base_types::SuiAddress;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::ObjectByTypeKey;
use sui_types::storage::OwnedObjectKey;
use sui_types::storage::OwnedObjectOwner;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::ledger_history::cursor::proto::sui::rpc::cursor::v1 as grpc;

use super::checkpoint::PinSource;

const ORDERING_ASCENDING: i32 = ProtoOrdering::Ascending as i32;
const ORDERING_DESCENDING: i32 = ProtoOrdering::Descending as i32;

/// Listing-scoped sort key carried inside a checkpoint-pinned `StateService` cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StateCursorKey {
    Balances {
        owner: SuiAddress,
        coin_type: StructTag,
    },
    OwnedObjects {
        owner: OwnedObjectOwner,
        key: OwnedObjectKey,
    },
    DynamicFields {
        key: DynamicFieldKey,
    },
    Objects {
        key: ObjectByTypeKey,
    },
}

/// Checkpoint-pinned `StateService` cursor, serialized as the `state` position variant of
/// `sui.rpc.cursor.v1.CursorToken`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StateCursor {
    pub(super) checkpoint: u64,
    pub(super) key: StateCursorKey,
}

/// Upper bound on encoded cursor / page-token byte length to bound BCS `TypeTag` allocations.
const MAX_CURSOR_BYTES: usize = 64 * 1024;

impl StateCursor {
    pub(super) fn encode(&self) -> Bytes {
        let state = match &self.key {
            StateCursorKey::Balances { owner, coin_type } => grpc::StatePosition {
                checkpoint: Some(self.checkpoint),
                listing: grpc::StateListing::Balances as i32,
                owner_kind: grpc::StateOwnerKind::Unspecified as i32,
                owner: Some(owner.to_vec()),
                move_type: Some(bcs::to_bytes(coin_type).expect("StructTag BCS encodes")),
                balance: None,
                object_id: None,
            },
            StateCursorKey::OwnedObjects { owner, key } => {
                let (owner_kind, owner_bytes) = match owner {
                    OwnedObjectOwner::Address(addr) => {
                        (grpc::StateOwnerKind::Address, Some(addr.to_vec()))
                    }
                    OwnedObjectOwner::Object(id) => {
                        (grpc::StateOwnerKind::Object, Some(id.to_vec()))
                    }
                    OwnedObjectOwner::Shared => (grpc::StateOwnerKind::Shared, None),
                    OwnedObjectOwner::Immutable => (grpc::StateOwnerKind::Immutable, None),
                };
                grpc::StatePosition {
                    checkpoint: Some(self.checkpoint),
                    listing: grpc::StateListing::OwnedObjects as i32,
                    owner_kind: owner_kind as i32,
                    owner: owner_bytes,
                    move_type: Some(
                        bcs::to_bytes(&key.object_type).expect("StructTag BCS encodes"),
                    ),
                    balance: key.balance,
                    object_id: Some(key.object_id.to_vec()),
                }
            }
            StateCursorKey::DynamicFields { key } => grpc::StatePosition {
                checkpoint: Some(self.checkpoint),
                listing: grpc::StateListing::DynamicFields as i32,
                owner_kind: grpc::StateOwnerKind::Unspecified as i32,
                owner: Some(key.parent.to_vec()),
                move_type: Some(bcs::to_bytes(&key.object_type).expect("StructTag BCS encodes")),
                balance: key.balance,
                object_id: Some(key.field_id.to_vec()),
            },
            StateCursorKey::Objects { key } => grpc::StatePosition {
                checkpoint: Some(self.checkpoint),
                listing: grpc::StateListing::Objects as i32,
                owner_kind: grpc::StateOwnerKind::Unspecified as i32,
                owner: None,
                move_type: Some(bcs::to_bytes(&key.object_type).expect("StructTag BCS encodes")),
                balance: None,
                object_id: Some(key.object_id.to_vec()),
            },
        };

        grpc::CursorToken {
            kind: grpc::CursorKind::Item as i32,
            public_position: None,
            position: Some(grpc::cursor_token::Position::State(state)),
        }
        .encode_to_vec()
        .into()
    }

    pub(super) fn decode(field: &str, bytes: &[u8]) -> Result<Self> {
        let invalid = |msg: &str| -> RpcError {
            FieldViolation::new(field)
                .with_description(msg)
                .with_reason(ErrorReason::FieldInvalid)
                .into()
        };

        if bytes.len() > MAX_CURSOR_BYTES {
            return Err(invalid("cursor exceeds maximum length"));
        }
        let proto = grpc::CursorToken::decode(bytes).map_err(|_| invalid("invalid cursor"))?;
        if proto.kind != grpc::CursorKind::Item as i32 || proto.public_position.is_some() {
            return Err(invalid("invalid cursor"));
        }

        let Some(grpc::cursor_token::Position::State(state)) = proto.position else {
            return Err(invalid("invalid cursor"));
        };

        let Some(checkpoint) = state.checkpoint else {
            return Err(invalid("cursor missing checkpoint"));
        };

        let move_type_bytes = state
            .move_type
            .as_deref()
            .ok_or_else(|| invalid("cursor missing type"))?;
        // Cap one level below `MAX_CONTAINER_DEPTH` so `ListBalances` can wrap the `StructTag` in
        // `TypeTag::Struct` without overflowing BCS depth during index-key re-encoding.
        let move_type: StructTag =
            bcs::from_bytes_with_limit(move_type_bytes, bcs::MAX_CONTAINER_DEPTH - 1)
                .map_err(|_| invalid("invalid cursor type"))?;
        let listing = grpc::StateListing::try_from(state.listing)
            .map_err(|_| invalid("invalid cursor listing"))?;

        let key = match listing {
            grpc::StateListing::Balances => {
                if state.owner_kind != grpc::StateOwnerKind::Unspecified as i32
                    || state.balance.is_some()
                    || state.object_id.is_some()
                {
                    return Err(invalid("invalid balance cursor"));
                }
                let owner = decode_address(state.owner.as_deref(), field)?;
                StateCursorKey::Balances {
                    owner,
                    coin_type: move_type,
                }
            }
            grpc::StateListing::OwnedObjects => {
                let owner_kind = grpc::StateOwnerKind::try_from(state.owner_kind)
                    .map_err(|_| invalid("invalid owned-object cursor owner kind"))?;
                let owner = match owner_kind {
                    grpc::StateOwnerKind::Address => {
                        OwnedObjectOwner::Address(decode_address(state.owner.as_deref(), field)?)
                    }
                    grpc::StateOwnerKind::Object => {
                        OwnedObjectOwner::Object(decode_object_id(state.owner.as_deref(), field)?)
                    }
                    grpc::StateOwnerKind::Shared => {
                        if state.owner.is_some() {
                            return Err(invalid("invalid shared-object cursor"));
                        }
                        OwnedObjectOwner::Shared
                    }
                    grpc::StateOwnerKind::Immutable => {
                        if state.owner.is_some() {
                            return Err(invalid("invalid immutable-object cursor"));
                        }
                        OwnedObjectOwner::Immutable
                    }
                    grpc::StateOwnerKind::Unspecified => {
                        return Err(invalid("missing owned-object cursor owner kind"));
                    }
                };
                let object_id = decode_object_id(state.object_id.as_deref(), field)?;
                StateCursorKey::OwnedObjects {
                    owner,
                    key: OwnedObjectKey {
                        object_type: move_type,
                        balance: state.balance,
                        object_id,
                    },
                }
            }
            grpc::StateListing::DynamicFields => {
                if state.owner_kind != grpc::StateOwnerKind::Unspecified as i32 {
                    return Err(invalid("invalid dynamic-field cursor"));
                }
                let parent = decode_object_id(state.owner.as_deref(), field)?;
                let field_id = decode_object_id(state.object_id.as_deref(), field)?;
                StateCursorKey::DynamicFields {
                    key: DynamicFieldKey {
                        parent,
                        field_id,
                        object_type: move_type,
                        balance: state.balance,
                    },
                }
            }
            grpc::StateListing::Objects => {
                if state.owner_kind != grpc::StateOwnerKind::Unspecified as i32
                    || state.owner.is_some()
                    || state.balance.is_some()
                {
                    return Err(invalid("invalid objects cursor"));
                }
                let object_id = decode_object_id(state.object_id.as_deref(), field)?;
                StateCursorKey::Objects {
                    key: ObjectByTypeKey {
                        object_type: move_type,
                        object_id,
                    },
                }
            }
            grpc::StateListing::Unspecified => {
                return Err(invalid("missing cursor listing"));
            }
        };

        Ok(Self { checkpoint, key })
    }
}

fn decode_address(bytes: Option<&[u8]>, field: &str) -> Result<SuiAddress> {
    let raw = bytes.ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("cursor missing owner")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let arr: [u8; SUI_ADDRESS_LENGTH] = raw.try_into().map_err(|_| {
        FieldViolation::new(field)
            .with_description("invalid cursor owner length")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(SuiAddress::from(ObjectID::new(arr)))
}

fn decode_object_id(bytes: Option<&[u8]>, field: &str) -> Result<ObjectID> {
    let raw = bytes.ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("cursor missing object id")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let arr: [u8; ObjectID::LENGTH] = raw.try_into().map_err(|_| {
        FieldViolation::new(field)
            .with_description("invalid cursor object id length")
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    Ok(ObjectID::new(arr))
}

/// Validated `QueryOptions` for a checkpoint-pinned `StateService` listing.
#[derive(Clone, Debug)]
pub(super) struct StateQueryOptions<K> {
    pub(super) limit: usize,
    pub(super) descending: bool,
    pub(super) after: Option<K>,
    pub(super) before: Option<K>,
    pub(super) pin: PinSource,
}

impl<K> StateQueryOptions<K> {
    pub(super) fn into_bounds(self) -> (Bound<K>, Bound<K>, QueryEndReason) {
        let exhausted_end_reason = self.exhausted_end_reason();
        let lower = self.after.map_or(Bound::Unbounded, Bound::Excluded);
        let upper = self.before.map_or(Bound::Unbounded, Bound::Excluded);
        (lower, upper, exhausted_end_reason)
    }
    /// End reason when the listing exhausts its interval without hitting `limit` (or response size
    /// cap).
    pub(super) fn exhausted_end_reason(&self) -> QueryEndReason {
        let hit_far_cursor = if self.descending {
            self.after.is_some()
        } else {
            self.before.is_some()
        };
        if hit_far_cursor {
            QueryEndReason::CursorBound
        } else {
            QueryEndReason::CheckpointBound
        }
    }
}

/// Reject legacy `page_size` / `page_token` when `options` is set.
pub(super) fn reject_legacy_paging_with_options(
    page_size: Option<u32>,
    page_token: Option<&[u8]>,
) -> Result<()> {
    if page_size.is_some() {
        return Err(FieldViolation::new("page_size")
            .with_description("page_size and options are mutually exclusive")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    if page_token.is_some() {
        return Err(FieldViolation::new("page_token")
            .with_description("page_token and options are mutually exclusive")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    Ok(())
}

/// Parse and validate `QueryOptions` together with an optional `at_checkpoint` for a listing
/// whose scope is verified by `extract_key`.
pub(super) fn parse_query_options<K, F>(
    options: Option<&ProtoQueryOptions>,
    at_checkpoint: Option<u64>,
    default_limit: usize,
    max_limit: usize,
    extract_key: F,
) -> Result<StateQueryOptions<K>>
where
    F: Fn(&str, StateCursorKey) -> Result<K>,
{
    let limit = options
        .and_then(|o| o.limit)
        .map(|l| (l as usize).clamp(1, max_limit))
        .unwrap_or(default_limit);

    let descending = match options.and_then(|o| o.ordering) {
        None | Some(ORDERING_ASCENDING) => false,
        Some(ORDERING_DESCENDING) => true,
        Some(_) => {
            return Err(FieldViolation::new("options.ordering")
                .with_description("invalid ordering")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    };

    let after = options
        .and_then(|o| o.after.as_deref())
        .map(|bytes| {
            let cursor = StateCursor::decode("options.after", bytes)?;
            let key = extract_key("options.after", cursor.key)?;
            Ok::<_, RpcError>((cursor.checkpoint, key))
        })
        .transpose()?;

    let before = options
        .and_then(|o| o.before.as_deref())
        .map(|bytes| {
            let cursor = StateCursor::decode("options.before", bytes)?;
            let key = extract_key("options.before", cursor.key)?;
            Ok::<_, RpcError>((cursor.checkpoint, key))
        })
        .transpose()?;

    let cursor_checkpoint = match (&after, &before) {
        (Some((after_cp, _)), Some((before_cp, _))) => {
            if after_cp != before_cp {
                return Err(FieldViolation::new("options.before")
                    .with_description(format!(
                        "after cursor checkpoint {after_cp} does not match before cursor \
                         checkpoint {before_cp}"
                    ))
                    .with_reason(ErrorReason::FieldInvalid)
                    .into());
            }
            Some(*after_cp)
        }
        (Some((cp, _)), None) | (None, Some((cp, _))) => Some(*cp),
        (None, None) => None,
    };

    if let (Some(req_cp), Some(cur_cp)) = (at_checkpoint, cursor_checkpoint)
        && req_cp != cur_cp
    {
        let field = if after.is_some() {
            "options.after"
        } else {
            "options.before"
        };
        return Err(FieldViolation::new(field)
            .with_description(format!(
                "cursor checkpoint {cur_cp} does not match at_checkpoint {req_cp}"
            ))
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    let pin = match (at_checkpoint, cursor_checkpoint) {
        (Some(cp), _) => PinSource::AtCheckpoint(cp),
        (None, Some(cp)) => PinSource::Cursor(cp),
        (None, None) => PinSource::Latest,
    };

    Ok(StateQueryOptions {
        limit,
        descending,
        after: after.map(|(_, k)| k),
        before: before.map(|(_, k)| k),
        pin,
    })
}

/// Decode an optional `page_token` together with `at_checkpoint`, returning `(pin, lower_bound)`.
///
/// Treats `bytes` as a legacy bare-BCS token only if strict `bcs::from_bytes` (consuming every
/// byte) succeeds AND `legacy_matches` accepts the decoded owner/parent. Anything else must
/// decode as a `CursorToken` carrying a `StatePosition` with its checkpoint set and matching the
/// listing's scope and `at_checkpoint`, or `INVALID_ARGUMENT`.
pub(super) fn decode_page_token<L, K, LegacyMatches, LegacyKey, ExtractKey>(
    bytes: Option<&[u8]>,
    at_checkpoint: Option<u64>,
    legacy_matches: LegacyMatches,
    legacy_key: LegacyKey,
    extract_key: ExtractKey,
) -> Result<(Option<PinSource>, Bound<K>)>
where
    L: DeserializeOwned,
    LegacyMatches: FnOnce(&L) -> bool,
    LegacyKey: FnOnce(L) -> K,
    ExtractKey: FnOnce(&str, StateCursorKey) -> Result<K>,
{
    let Some(bytes) = bytes else {
        return Ok((at_checkpoint.map(PinSource::AtCheckpoint), Bound::Unbounded));
    };

    if bytes.len() > MAX_CURSOR_BYTES {
        return Err(FieldViolation::new("page_token")
            .with_description("page_token exceeds maximum length")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    if let Ok(legacy) = bcs::from_bytes::<L>(bytes)
        && legacy_matches(&legacy)
    {
        if at_checkpoint.is_some() {
            return Err(FieldViolation::new("page_token")
                .with_description("legacy page_token cannot be combined with at_checkpoint")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
        return Ok((None, Bound::Included(legacy_key(legacy))));
    }

    let cursor = StateCursor::decode("page_token", bytes)?;
    let key = extract_key("page_token", cursor.key)?;
    let pin = match at_checkpoint {
        Some(req_cp) if req_cp != cursor.checkpoint => {
            return Err(FieldViolation::new("page_token")
                .with_description(format!(
                    "page_token checkpoint {} does not match at_checkpoint {req_cp}",
                    cursor.checkpoint
                ))
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
        Some(req_cp) => PinSource::AtCheckpoint(req_cp),
        None => PinSource::Cursor(cursor.checkpoint),
    };
    Ok((Some(pin), Bound::Excluded(key)))
}

pub(super) fn wrong_cursor_scope(field: &str, message: &str) -> RpcError {
    FieldViolation::new(field)
        .with_description(message)
        .with_reason(ErrorReason::FieldInvalid)
        .into()
}

#[cfg(test)]
mod tests {
    use move_core_types::language_storage::TypeTag;

    use super::*;
    use crate::grpc::v2::state_service::test_support::struct_tag as tag;
    use crate::grpc::v2::state_service::test_support::test_address as addr;
    use crate::ledger_history::cursor::CursorToken as LedgerCursorToken;
    use crate::ledger_history::cursor::Position as LedgerPosition;

    #[test]
    fn roundtrips_all_state_cursor_variants() {
        let cases = [
            StateCursor {
                checkpoint: 0,
                key: StateCursorKey::Balances {
                    owner: addr(1),
                    coin_type: tag("0x2::sui::SUI"),
                },
            },
            StateCursor {
                checkpoint: 42,
                key: StateCursorKey::OwnedObjects {
                    owner: OwnedObjectOwner::Address(addr(2)),
                    key: OwnedObjectKey {
                        object_type: tag("0x2::coin::Coin<0x2::sui::SUI>"),
                        balance: Some(1_000),
                        object_id: ObjectID::from_single_byte(3),
                    },
                },
            },
            StateCursor {
                checkpoint: 43,
                key: StateCursorKey::OwnedObjects {
                    owner: OwnedObjectOwner::Object(ObjectID::from_single_byte(4)),
                    key: OwnedObjectKey {
                        object_type: tag("0x1::a::A"),
                        balance: None,
                        object_id: ObjectID::from_single_byte(5),
                    },
                },
            },
            StateCursor {
                checkpoint: 44,
                key: StateCursorKey::OwnedObjects {
                    owner: OwnedObjectOwner::Shared,
                    key: OwnedObjectKey {
                        object_type: tag("0x2::kiosk::Kiosk"),
                        balance: None,
                        object_id: ObjectID::from_single_byte(6),
                    },
                },
            },
            StateCursor {
                checkpoint: 45,
                key: StateCursorKey::OwnedObjects {
                    owner: OwnedObjectOwner::Immutable,
                    key: OwnedObjectKey {
                        object_type: tag("0x2::coin::CoinMetadata<0x2::sui::SUI>"),
                        balance: None,
                        object_id: ObjectID::from_single_byte(7),
                    },
                },
            },
            StateCursor {
                checkpoint: 46,
                key: StateCursorKey::DynamicFields {
                    key: DynamicFieldKey {
                        parent: ObjectID::from_single_byte(8),
                        field_id: ObjectID::from_single_byte(9),
                        object_type: tag("0x2::dynamic_field::Field<u64, u64>"),
                        balance: None,
                    },
                },
            },
            StateCursor {
                checkpoint: 46,
                key: StateCursorKey::DynamicFields {
                    key: DynamicFieldKey {
                        parent: ObjectID::from_single_byte(8),
                        field_id: ObjectID::from_single_byte(11),
                        object_type: tag("0x2::coin::Coin<0x2::sui::SUI>"),
                        balance: Some(500),
                    },
                },
            },
            StateCursor {
                checkpoint: 47,
                key: StateCursorKey::Objects {
                    key: ObjectByTypeKey {
                        object_type: tag("0x2::kiosk::Kiosk"),
                        object_id: ObjectID::from_single_byte(10),
                    },
                },
            },
        ];

        for cursor in cases {
            let encoded = cursor.encode();
            let decoded = StateCursor::decode("options.after", &encoded).unwrap();
            assert_eq!(decoded, cursor);
            // Ledger-history cursor decoder must reject every StateService cursor.
            assert!(LedgerCursorToken::decode(&encoded).is_err());
        }
    }

    #[test]
    fn rejects_empty_missing_checkpoint_and_ledger_cursors() {
        // Empty protobuf message has no position and no kind.
        assert_eq!(
            StateCursor::decode("options.after", &[])
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // StatePosition without checkpoint must be rejected.
        let missing_cp = grpc::CursorToken {
            kind: grpc::CursorKind::Item as i32,
            public_position: None,
            position: Some(grpc::cursor_token::Position::State(grpc::StatePosition {
                checkpoint: None,
                listing: grpc::StateListing::Objects as i32,
                owner_kind: grpc::StateOwnerKind::Unspecified as i32,
                owner: None,
                move_type: Some(bcs::to_bytes(&tag("0x1::a::A")).unwrap()),
                balance: None,
                object_id: Some(ObjectID::from_single_byte(1).to_vec()),
            })),
        }
        .encode_to_vec();
        assert_eq!(
            StateCursor::decode("options.after", &missing_cp)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // Ledger-history cursor must be rejected.
        let ledger =
            LedgerCursorToken::item(LedgerPosition::Checkpoints { checkpoint: 5 }).encode();
        assert_eq!(
            StateCursor::decode("options.after", &ledger)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
        // A depth-500 StructTag cannot be re-encoded inside TypeTag::Struct (depth 501) and must
        // be rejected as InvalidArgument rather than failing later with Internal.
        let make_nested_tag = |vector_layers: usize| {
            let mut inner = TypeTag::Bool;
            for _ in 0..vector_layers {
                inner = TypeTag::Vector(Box::new(inner));
            }
            let mut base = tag("0x2::coin::Coin<0x2::sui::SUI>");
            base.type_params = vec![inner];
            base
        };
        let wrap_balance_cursor = |move_type: &StructTag| {
            grpc::CursorToken {
                kind: grpc::CursorKind::Item as i32,
                public_position: None,
                position: Some(grpc::cursor_token::Position::State(grpc::StatePosition {
                    checkpoint: Some(1),
                    listing: grpc::StateListing::Balances as i32,
                    owner_kind: grpc::StateOwnerKind::Unspecified as i32,
                    owner: Some(addr(1).to_vec()),
                    move_type: Some(bcs::to_bytes(move_type).unwrap()),
                    balance: None,
                    object_id: None,
                })),
            }
            .encode_to_vec()
        };
        // StructTag(1) + 498 Vector layers + Bool(1) = depth 500 -> rejected.
        let depth_500 = wrap_balance_cursor(&make_nested_tag(498));
        assert_eq!(
            StateCursor::decode("options.after", &depth_500)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
        // StructTag(1) + 497 Vector layers + Bool(1) = depth 499 -> accepted and re-encodes inside
        // TypeTag::Struct (depth 500).
        let depth_499 = wrap_balance_cursor(&make_nested_tag(497));
        let decoded = StateCursor::decode("options.after", &depth_499).unwrap();
        let StateCursorKey::Balances { coin_type, .. } = decoded.key else {
            panic!("expected Balances cursor");
        };
        assert!(bcs::to_bytes(&TypeTag::Struct(Box::new(coin_type))).is_ok());
    }

    #[test]
    fn legacy_page_token_requires_full_consumption_and_matching_owner() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
        struct LegacyBalance {
            owner: SuiAddress,
            coin_type: StructTag,
        }

        let owner = addr(7);
        let other = addr(8);
        let legacy = LegacyBalance {
            owner,
            coin_type: tag("0x2::sui::SUI"),
        };
        let bytes = bcs::to_bytes(&legacy).unwrap();

        // Matching owner, unpinned -> (None, Included(coin_type)).
        let (pin, lower) = decode_page_token::<LegacyBalance, StructTag, _, _, _>(
            Some(&bytes),
            None,
            |t| t.owner == owner,
            |t| t.coin_type,
            |f, _| Err(wrong_cursor_scope(f, "wrong")),
        )
        .unwrap();
        assert_eq!(pin, None);
        assert_eq!(lower, Bound::Included(legacy.coin_type.clone()));

        // Trailing garbage byte fails strict BCS and falls through to CursorToken -> InvalidArgument.
        let mut with_trailing = bytes.clone();
        with_trailing.push(0);
        assert_eq!(
            decode_page_token::<LegacyBalance, StructTag, _, _, _>(
                Some(&with_trailing),
                None,
                |t| t.owner == owner,
                |t| t.coin_type,
                |f, _| Err(wrong_cursor_scope(f, "wrong")),
            )
            .err()
            .unwrap()
            .code(),
            tonic::Code::InvalidArgument
        );

        // Different owner fails legacy match and falls through -> InvalidArgument.
        assert_eq!(
            decode_page_token::<LegacyBalance, StructTag, _, _, _>(
                Some(&bytes),
                None,
                |t| t.owner == other,
                |t| t.coin_type,
                |f, _| Err(wrong_cursor_scope(f, "wrong")),
            )
            .err()
            .unwrap()
            .code(),
            tonic::Code::InvalidArgument
        );

        // Legacy token combined with at_checkpoint -> InvalidArgument.
        assert_eq!(
            decode_page_token::<LegacyBalance, StructTag, _, _, _>(
                Some(&bytes),
                Some(10),
                |t| t.owner == owner,
                |t| t.coin_type,
                |f, _| Err(wrong_cursor_scope(f, "wrong")),
            )
            .err()
            .unwrap()
            .code(),
            tonic::Code::InvalidArgument
        );

        // Pinned CursorToken decodes as pinned and validates checkpoint match.
        let pinned_bytes = StateCursor {
            checkpoint: 10,
            key: StateCursorKey::Balances {
                owner,
                coin_type: tag("0x2::sui::SUI"),
            },
        }
        .encode();
        let extract = |field: &str, key: StateCursorKey| match key {
            StateCursorKey::Balances {
                owner: cur_owner,
                coin_type,
            } if cur_owner == owner => Ok(coin_type),
            _ => Err(wrong_cursor_scope(field, "wrong scope")),
        };
        let (pin, lower) = decode_page_token::<LegacyBalance, StructTag, _, _, _>(
            Some(&pinned_bytes),
            Some(10),
            |t| t.owner == owner,
            |t| t.coin_type,
            extract,
        )
        .unwrap();
        assert_eq!(pin, Some(PinSource::AtCheckpoint(10)));
        assert_eq!(lower, Bound::Excluded(tag("0x2::sui::SUI")));

        // Without at_checkpoint, pinned CursorToken pins with PinSource::Cursor.
        let (pin, _) = decode_page_token::<LegacyBalance, StructTag, _, _, _>(
            Some(&pinned_bytes),
            None,
            |t| t.owner == owner,
            |t| t.coin_type,
            extract,
        )
        .unwrap();
        assert_eq!(pin, Some(PinSource::Cursor(10)));

        // Mismatched at_checkpoint -> InvalidArgument.
        assert_eq!(
            decode_page_token::<LegacyBalance, StructTag, _, _, _>(
                Some(&pinned_bytes),
                Some(11),
                |t| t.owner == owner,
                |t| t.coin_type,
                extract,
            )
            .err()
            .unwrap()
            .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn parse_query_options_validates_checkpoint_and_ordering_rules() {
        let owner = addr(1);
        let mk = |cp: u64, name: &str| {
            StateCursor {
                checkpoint: cp,
                key: StateCursorKey::Balances {
                    owner,
                    coin_type: tag(&format!("0x2::coin::{name}")),
                },
            }
            .encode()
        };
        let extract = |field: &str, key: StateCursorKey| match key {
            StateCursorKey::Balances {
                owner: cur_owner,
                coin_type,
            } if cur_owner == owner => Ok(coin_type),
            _ => Err(wrong_cursor_scope(field, "wrong scope")),
        };

        // Different checkpoints between after and before -> InvalidArgument.
        let mut opts = ProtoQueryOptions::default();
        opts.after = Some(mk(10, "A"));
        opts.before = Some(mk(11, "B"));
        assert_eq!(
            parse_query_options(Some(&opts), None, 50, 1000, extract)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );

        // Matching checkpoints -> Cursor source when at_checkpoint is None.
        let mut opts = ProtoQueryOptions::default();
        opts.after = Some(mk(10, "A"));
        opts.before = Some(mk(10, "B"));
        opts.ordering = Some(ORDERING_DESCENDING);
        opts.limit = Some(5000);
        let parsed = parse_query_options(Some(&opts), None, 50, 1000, extract).unwrap();
        assert_eq!(parsed.pin, PinSource::Cursor(10));
        assert_eq!(parsed.limit, 1000);
        assert!(parsed.descending);
        assert_eq!(parsed.exhausted_end_reason(), QueryEndReason::CursorBound);

        // Invalid ordering -> InvalidArgument.
        let mut bad_ordering = ProtoQueryOptions::default();
        bad_ordering.ordering = Some(99);
        assert_eq!(
            parse_query_options(Some(&bad_ordering), None, 50, 1000, extract)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
    }

    #[test]
    fn rejects_oversized_cursor_and_page_token() {
        let oversized = vec![0u8; MAX_CURSOR_BYTES + 1];
        assert_eq!(
            StateCursor::decode("options.after", &oversized)
                .unwrap_err()
                .code(),
            tonic::Code::InvalidArgument
        );
        assert_eq!(
            decode_page_token::<Vec<u8>, (), _, _, _>(
                Some(&oversized),
                None,
                |_| true,
                |_| (),
                |_, _| Ok(()),
            )
            .unwrap_err()
            .code(),
            tonic::Code::InvalidArgument
        );
    }
}
