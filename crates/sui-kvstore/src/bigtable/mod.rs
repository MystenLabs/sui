// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod client;
// TODO(migration): Remove once GraphQL reads per-pipeline watermarks.
pub(crate) mod legacy_watermark;
mod metrics;
#[cfg(test)]
pub(crate) mod mock_server;
pub(crate) mod proto;
pub(crate) mod store;
