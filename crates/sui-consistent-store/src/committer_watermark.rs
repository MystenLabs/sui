// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Conversion helpers between [`Watermark`] (the on-disk
//! representation persisted in the framework's watermark CF) and
//! [`CommitterWatermark`] (the indexer-alt framework's
//! per-pipeline progress type).
//!
//! The two types share the same four `u64` fields. We can't
//! supply `From` impls because [`CommitterWatermark`] is not local
//! to this crate (orphan rule), so the conversion is exposed as
//! plain functions instead.

use sui_indexer_alt_framework_store_traits::CommitterWatermark;

use crate::Watermark;

/// Convert an on-disk [`Watermark`] into the framework's
/// [`CommitterWatermark`].
pub(crate) fn to_committer(w: Watermark) -> CommitterWatermark {
    CommitterWatermark {
        epoch_hi_inclusive: w.epoch_hi_inclusive,
        checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
        tx_hi: w.tx_hi,
        timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
    }
}

/// Convert a framework [`CommitterWatermark`] into the on-disk
/// [`Watermark`].
pub(crate) fn from_committer(w: CommitterWatermark) -> Watermark {
    Watermark {
        epoch_hi_inclusive: w.epoch_hi_inclusive,
        checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
        tx_hi: w.tx_hi,
        timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committer_watermark_conversion_is_bijective() {
        let w = Watermark {
            epoch_hi_inclusive: 7,
            checkpoint_hi_inclusive: 42,
            tx_hi: 1_000,
            timestamp_ms_hi_inclusive: 1_700_000_000_000,
        };
        let cw = to_committer(w);
        let back = from_committer(cw);
        assert_eq!(back, w);
    }
}
