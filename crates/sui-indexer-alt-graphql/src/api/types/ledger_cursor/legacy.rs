// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A frozen copy of the cursor wire format GraphQL issued before it owned its cursor format: the
//! ledger service's `CursorToken` protobuf message as GraphQL last minted it.
//!
//! GraphQL accepts these cursors so that cursors issued by earlier releases, and cursors a client got
//! straight from the ledger service, keep working. That includes the cursors Postgres-backed
//! pagination issued (labelled checkpoint 0), which clients may still hold. The ledger service is
//! handed them unchanged, except as a page's `before` cursor ([`LegacyToken::before_bytes`]).
//!
//! This copy never follows changes to the ledger service's format, and rejects anything it does not
//! recognize rather than guessing at a position.

use anyhow::Context as _;
use bytes::Bytes;
use prost::Message as _;

/// Whether a legacy cursor names a returned item or a scan frontier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyKind {
    /// A returned item: pages after it start strictly after it.
    Item,
    /// A scan frontier: pages after it start at it.
    Boundary,
}

/// The coordinates a legacy cursor carries. The checkpoint of a transaction or event cursor is a
/// label that can be 0 (unknown) or off by one; only its `tx_seq` places it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyPosition {
    Checkpoints {
        checkpoint: u64,
    },
    Transactions {
        checkpoint: u64,
        tx_seq: u64,
    },
    Events {
        checkpoint: u64,
        tx_seq: u64,
        event_index: u32,
    },
}

/// A decoded legacy cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyToken {
    pub kind: LegacyKind,
    pub position: LegacyPosition,
}

/// The `CursorToken` message: `kind` and a `position` oneof over tags 6-8.
#[derive(Clone, Copy, PartialEq, prost::Message)]
struct WireToken {
    /// 1 = item, 2 = boundary.
    #[prost(int32, tag = "5")]
    kind: i32,
    #[prost(oneof = "WirePosition", tags = "6, 7, 8")]
    position: Option<WirePosition>,
}

#[derive(Clone, Copy, PartialEq, prost::Oneof)]
enum WirePosition {
    #[prost(message, tag = "6")]
    Checkpoints(WireCheckpoints),
    #[prost(message, tag = "7")]
    Transactions(WireTransactions),
    #[prost(message, tag = "8")]
    Events(WireEvents),
}

#[derive(Clone, Copy, PartialEq, prost::Message)]
struct WireCheckpoints {
    #[prost(uint64, optional, tag = "1")]
    checkpoint: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, prost::Message)]
struct WireTransactions {
    #[prost(uint64, optional, tag = "1")]
    checkpoint: Option<u64>,
    #[prost(uint64, optional, tag = "2")]
    tx_seq: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, prost::Message)]
struct WireEvents {
    #[prost(uint64, optional, tag = "1")]
    checkpoint: Option<u64>,
    #[prost(uint64, optional, tag = "2")]
    tx_seq: Option<u64>,
    #[prost(uint32, optional, tag = "3")]
    event_index: Option<u32>,
}

impl LegacyToken {
    /// Decode a `CursorToken` message.
    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        Self::try_from(WireToken::decode(bytes)?)
    }

    /// Encode as a `WireToken`, the schema GraphQL last minted.
    pub fn encode(&self) -> Bytes {
        let kind = match self.kind {
            LegacyKind::Item => 1,
            LegacyKind::Boundary => 2,
        };

        let position = match self.position {
            LegacyPosition::Checkpoints { checkpoint } => {
                WirePosition::Checkpoints(WireCheckpoints {
                    checkpoint: Some(checkpoint),
                })
            }

            LegacyPosition::Transactions { checkpoint, tx_seq } => {
                WirePosition::Transactions(WireTransactions {
                    checkpoint: Some(checkpoint),
                    tx_seq: Some(tx_seq),
                })
            }

            LegacyPosition::Events {
                checkpoint,
                tx_seq,
                event_index,
            } => WirePosition::Events(WireEvents {
                checkpoint: Some(checkpoint),
                tx_seq: Some(tx_seq),
                event_index: Some(event_index),
            }),
        };

        WireToken {
            kind,
            position: Some(position),
        }
        .encode_to_vec()
        .into()
    }

    /// The bytes to send the ledger service when this cursor, received as `bytes`, is a page's
    /// `before` cursor.
    ///
    /// Postgres-backed pagination labels its transaction and event cursors with checkpoint 0.
    /// Ledger servers before 1.83 read a `before` cursor's label as the end of the scan's
    /// checkpoint range, so that label would end every scan at checkpoint 1. Such a cursor is sent
    /// labelled `u64::MAX` instead, which leaves the range to the request. The first transaction
    /// and its first event really are in checkpoint 0 and keep their label, as does every other
    /// cursor, which is sent as received.
    pub fn before_bytes(&self, bytes: &Bytes) -> Bytes {
        let position = match self.position {
            LegacyPosition::Transactions {
                checkpoint: 0,
                tx_seq,
            } if tx_seq != 0 => LegacyPosition::Transactions {
                checkpoint: u64::MAX,
                tx_seq,
            },

            LegacyPosition::Events {
                checkpoint: 0,
                tx_seq,
                event_index,
            } if tx_seq != 0 || event_index != 0 => LegacyPosition::Events {
                checkpoint: u64::MAX,
                tx_seq,
                event_index,
            },

            _ => return bytes.clone(),
        };

        Self {
            kind: self.kind,
            position,
        }
        .encode()
    }
}

impl LegacyKind {
    fn from_wire(kind: i32) -> anyhow::Result<Self> {
        match kind {
            1 => Ok(LegacyKind::Item),
            2 => Ok(LegacyKind::Boundary),
            k => anyhow::bail!("unknown cursor kind: {k}"),
        }
    }
}

/// A transaction or event cursor without a checkpoint label decodes with checkpoint 0, the label
/// cursors minted without a known checkpoint carry.
impl TryFrom<WireToken> for LegacyToken {
    type Error = anyhow::Error;

    fn try_from(token: WireToken) -> anyhow::Result<Self> {
        let kind = LegacyKind::from_wire(token.kind)?;
        let position = match token.position.context("cursor missing position")? {
            WirePosition::Checkpoints(p) => LegacyPosition::Checkpoints {
                checkpoint: p.checkpoint.context("cursor missing checkpoint")?,
            },

            WirePosition::Transactions(p) => LegacyPosition::Transactions {
                checkpoint: p.checkpoint.unwrap_or(0),
                tx_seq: p.tx_seq.context("cursor missing tx_seq")?,
            },

            WirePosition::Events(p) => LegacyPosition::Events {
                checkpoint: p.checkpoint.unwrap_or(0),
                tx_seq: p.tx_seq.context("cursor missing tx_seq")?,
                event_index: p.event_index.context("cursor missing event_index")?,
            },
        };

        Ok(Self { kind, position })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cursors byte for byte as GraphQL last minted this format, in every position variant and both
    /// kinds: decoding reads them, and encoding reproduces them.
    #[test]
    fn round_trips_the_minted_bytes() {
        let cases: [(&[u8], LegacyToken); 5] = [
            // kind=ITEM, checkpoints { checkpoint: 7 }.
            (
                &[0x28, 0x01, 0x32, 0x02, 0x08, 0x07],
                LegacyToken {
                    kind: LegacyKind::Item,
                    position: LegacyPosition::Checkpoints { checkpoint: 7 },
                },
            ),
            // kind=BOUNDARY, transactions { checkpoint: 9, tx_seq: 10 }.
            (
                &[0x28, 0x02, 0x3a, 0x04, 0x08, 0x09, 0x10, 0x0a],
                LegacyToken {
                    kind: LegacyKind::Boundary,
                    position: LegacyPosition::Transactions {
                        checkpoint: 9,
                        tx_seq: 10,
                    },
                },
            ),
            // kind=ITEM, events { checkpoint: 11, tx_seq: 12, event_index: 13 }.
            (
                &[0x28, 0x01, 0x42, 0x06, 0x08, 0x0b, 0x10, 0x0c, 0x18, 0x0d],
                LegacyToken {
                    kind: LegacyKind::Item,
                    position: LegacyPosition::Events {
                        checkpoint: 11,
                        tx_seq: 12,
                        event_index: 13,
                    },
                },
            ),
            // kind=ITEM, transactions { checkpoint: 0, tx_seq: 10 }, as Postgres-backed pagination
            // issued it.
            (
                &[0x28, 0x01, 0x3a, 0x04, 0x08, 0x00, 0x10, 0x0a],
                LegacyToken {
                    kind: LegacyKind::Item,
                    position: LegacyPosition::Transactions {
                        checkpoint: 0,
                        tx_seq: 10,
                    },
                },
            ),
            // kind=ITEM, events { checkpoint: 0, tx_seq: 12, event_index: 13 }, as Postgres-backed
            // pagination issued it.
            (
                &[0x28, 0x01, 0x42, 0x06, 0x08, 0x00, 0x10, 0x0c, 0x18, 0x0d],
                LegacyToken {
                    kind: LegacyKind::Item,
                    position: LegacyPosition::Events {
                        checkpoint: 0,
                        tx_seq: 12,
                        event_index: 13,
                    },
                },
            ),
        ];

        for (bytes, token) in cases {
            assert_eq!(LegacyToken::decode(bytes).unwrap(), token);
            assert_eq!(token.encode().as_ref(), bytes);
        }
    }

    /// A transaction cursor without a checkpoint label reads as checkpoint 0.
    #[test]
    fn missing_checkpoint_label_reads_as_zero() {
        // kind=ITEM, transactions { tx_seq: 10 }.
        let bytes = [0x28, 0x01, 0x3a, 0x02, 0x10, 0x0a];
        assert_eq!(
            LegacyToken::decode(&bytes).unwrap().position,
            LegacyPosition::Transactions {
                checkpoint: 0,
                tx_seq: 10,
            },
        );
    }

    /// A cursor kind this copy does not know (such as one the ledger service adds later) fails to
    /// decode instead of being read as an item or a boundary.
    #[test]
    fn rejects_unknown_kinds() {
        for kind in [0, 3] {
            let bytes = WireToken {
                kind,
                position: Some(WirePosition::Transactions(WireTransactions {
                    checkpoint: Some(1),
                    tx_seq: Some(2),
                })),
            }
            .encode_to_vec();
            assert!(LegacyToken::decode(&bytes).is_err(), "kind {kind}");
        }
    }

    /// Missing coordinates fail to decode rather than defaulting to position 0.
    #[test]
    fn rejects_missing_coordinates() {
        let no_position = WireToken {
            kind: 1,
            position: None,
        };
        let no_tx_seq = WireToken {
            kind: 1,
            position: Some(WirePosition::Transactions(WireTransactions {
                checkpoint: Some(1),
                tx_seq: None,
            })),
        };
        for token in [no_position, no_tx_seq] {
            assert!(LegacyToken::decode(&token.encode_to_vec()).is_err());
        }

        assert!(LegacyToken::decode(&[]).is_err());
    }

    /// As a `before` cursor, a transaction or event cursor labelled checkpoint 0 is sent labelled
    /// `u64::MAX` at the same place. The cursor of the first transaction or its first event, a
    /// labelled cursor, and a checkpoint cursor are sent as received.
    #[test]
    fn before_bytes_relabel_checkpoint_zero() {
        let transaction = |checkpoint, tx_seq| LegacyToken {
            kind: LegacyKind::Boundary,
            position: LegacyPosition::Transactions { checkpoint, tx_seq },
        };
        let event = |checkpoint, tx_seq, event_index| LegacyToken {
            kind: LegacyKind::Item,
            position: LegacyPosition::Events {
                checkpoint,
                tx_seq,
                event_index,
            },
        };

        fn sent_before(token: LegacyToken) -> LegacyToken {
            LegacyToken::decode(&token.before_bytes(&token.encode())).unwrap()
        }

        assert_eq!(sent_before(transaction(0, 10)), transaction(u64::MAX, 10));
        assert_eq!(sent_before(event(0, 12, 0)), event(u64::MAX, 12, 0));
        assert_eq!(sent_before(event(0, 0, 3)), event(u64::MAX, 0, 3));

        let checkpoint = LegacyToken {
            kind: LegacyKind::Item,
            position: LegacyPosition::Checkpoints { checkpoint: 0 },
        };
        let received = Bytes::from_static(b"received");
        for unchanged in [
            transaction(0, 0),
            transaction(9, 10),
            event(0, 0, 0),
            event(11, 12, 13),
            checkpoint,
        ] {
            assert_eq!(unchanged.before_bytes(&received), received, "{unchanged:?}");
        }
    }
}
