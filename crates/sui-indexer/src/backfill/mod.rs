// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::ValueEnum;

pub mod backfill_instances;
pub mod backfill_runner;
pub mod backfill_task;

#[derive(ValueEnum, Clone, Debug)]
pub enum BackfillTaskKind {
    FullObjectsHistory,
    SystemStateSummaryJson,
}
