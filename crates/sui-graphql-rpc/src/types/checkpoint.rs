// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    base64::Base64, committee_member::CommitteeMember, end_of_epoch_data::EndOfEpochData,
    epoch::Epoch, gas::GasCostSummary,
};
use async_graphql::*;
use fastcrypto::traits::EncodeDecodeBase64;

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

impl From<&sui_json_rpc_types::Checkpoint> for Checkpoint {
    fn from(c: &sui_json_rpc_types::Checkpoint) -> Self {
        let end_of_epoch_data = &c.end_of_epoch_data;
        let end_of_epoch = end_of_epoch_data.clone().map(|e| {
            let committee: Vec<_> = e
                .next_epoch_committee
                .iter()
                .map(|(authority, stake)| CommitteeMember {
                    authority_name: Some(authority.into_concise().to_string()),
                    stake_unit: Some(*stake),
                })
                .collect();

            EndOfEpochData {
                new_committee: Some(committee),
                next_protocol_version: Some(e.next_epoch_protocol_version.as_u64()),
            }
        });

        Self {
            digest: c.digest.to_string(),
            sequence_number: c.sequence_number,
            validator_signature: Some(Base64::from(
                c.validator_signature.encode_base64().into_bytes(),
            )),
            previous_checkpoint_digest: c.previous_digest.map(|x| x.to_string()),
            live_object_set_digest: None, // TODO fix this
            network_total_transactions: Some(c.network_total_transactions),
            rolling_gas_summary: Some(GasCostSummary::from(&c.epoch_rolling_gas_cost_summary)),
            epoch: Some(Epoch {
                epoch_id: c.epoch,
                gas_cost_summary: Some(GasCostSummary::from(&c.epoch_rolling_gas_cost_summary)),
            }),
            end_of_epoch,
        }
    }
}
