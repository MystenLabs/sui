// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{base64::Base64, end_of_epoch_data::EndOfEpochData, epoch::Epoch, gas::GasCostSummary};
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct Checkpoint {
    // id: ID1,
    pub digest: String,
    pub sequence_number: u64,
    // timestamp: DateTime,
    pub validator_signature: Option<Base64>,
    pub previous_checkpoint_digest: Option<String>,
    pub live_object_set_digest: Option<String>,
    pub network_total_transactions: Option<u64>,
    pub rolling_gas_summary: Option<GasCostSummary>,
    pub epoch: Option<Epoch>,
    pub end_of_epoch: Option<EndOfEpochData>,
    // transactionConnection(first: Int, after: String, last: Int, before: String): TransactionBlockConnection
    // address_metrics: AddressMetrics,
}
