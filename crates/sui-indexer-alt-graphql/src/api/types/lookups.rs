// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::scalars::uint53::UInt53;

pub(crate) trait CheckpointBounds {
    fn after_checkpoint(&self) -> Option<UInt53>;
    fn at_checkpoint(&self) -> Option<UInt53>;
    fn before_checkpoint(&self) -> Option<UInt53>;
}

pub(crate) trait TxBoundsCursor {
    fn tx_sequence_number(&self) -> u64;
}
